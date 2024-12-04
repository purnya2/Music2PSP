use egui::ahash::HashMap;
use glob::glob;
use image::imageops::FilterType;
use image::{ImageFormat, ImageReader};
use mp3lame_encoder::*;
use std::borrow::Cow;
use std::default::Default;
use std::fmt::Formatter;
use std::fs::File;
use std::io::{Cursor, Write};
use std::path::PathBuf;
use std::{fmt, fs, thread};
use symphonia::core::audio::{AudioBuffer, AudioBufferRef, Signal};
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::conv::IntoSample;
use symphonia::core::errors::Error;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::{Metadata, MetadataOptions, StandardTagKey};
use symphonia::core::probe::Hint;
use symphonia::core::sample::{u24, Sample};

// TODO a hashset thingy maybe that will store the images
// so that I don't have to regenerate the images continuously
#[derive(serde::Deserialize, serde::Serialize, Debug)]

struct AlbumArtCache {
    album_art: HashMap<u64, Vec<u8>>,
}

impl Default for AlbumArtCache {
    fn default() -> Self {
        todo!()
    }
}

pub enum AudioFiletype {
    MP3,
    FLAC,
    OGG,

}
pub struct AudioConverter {
    from_type: AudioFiletype,
    to_type: AudioFiletype,
    src_path: PathBuf,
    output_based_on_metadata: bool,
}

struct TrackMetadata {
    title: String,
    track_number: String,
    artist: Vec<String>,
    album: String,
    album_art: Box<[u8]>,
    year: String,
    comment: String,
    sample_rate: u32,
}
impl fmt::Debug for TrackMetadata {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("TrackMetadata")
            .field("title", &self.title)
            .field("artist", &self.artist)
            .field("album", &self.album)
            .field("year", &self.year)
            .field("comment", &self.comment)
            .field("album_art length", &self.album_art.len())
            .finish()
    }
}

impl AudioConverter {
    pub(crate) fn new(src_path: PathBuf, to_type: AudioFiletype) -> Self {
        let from_type = src_path
            .extension()
            .and_then(|ext| ext.to_str())
            .and_then(|ext_str| match ext_str {
                "flac" => Some(AudioFiletype::FLAC),
                "mp3" => Some(AudioFiletype::MP3),
                "ogg" => Some(AudioFiletype::OGG),
                _ => None,
            })
            .unwrap_or_else(|| panic!("woah owah no file extension for {:?}", src_path));

        AudioConverter {
            src_path,
            from_type,
            to_type,
            output_based_on_metadata: true,
        }
    }

    fn __extract_metadata(&self, input_path: PathBuf) -> Result<TrackMetadata, Error> {
        let mut hint = Hint::new();
        if let Some(extension) = input_path.extension() {
            if let Some(extension_str) = extension.to_str() {
                println!("do I come here");
                hint.with_extension(extension_str);
            }
        }
        let src = File::open(input_path).expect("failed to open .flac file");

        let mss_src = MediaSourceStream::new(Box::new(src), Default::default());

        let meta_opts: MetadataOptions = Default::default();
        let fmt_opts: FormatOptions = Default::default();

        let mut probed = symphonia::default::get_probe()
            .format(&hint, mss_src, &fmt_opts, &meta_opts)
            .expect("unsupported format");

        let mut format = probed.format;

        if let Some(metadata_rev) = format.metadata().current() {
            let binding = format.metadata();
            self._extract_metadata(binding)
        } else if let Some(metadata_rev) = probed.metadata.get().as_ref().and_then(|m| m.current())
        {
            let binding = probed.metadata.get().unwrap();
            self._extract_metadata(binding)
        } else {
            Err(Error::Unsupported("no metadata found"))
        }
    }

    fn _extract_metadata(&self, binding: Metadata<'_>) -> Result<TrackMetadata, Error> {
        let metadata = binding.current().unwrap();

        let mut track_metadata: TrackMetadata = TrackMetadata {
            title: "".to_string(),
            track_number: "".to_string(),
            artist: vec![],
            album: "".to_string(),
            album_art: Box::new([]),
            year: "".to_string(),
            comment: "".to_string(),
            sample_rate: 44_100,
        };

        // estraggo i metadati
        for tag in metadata.tags().iter() {
            match tag.std_key {
                Some(key) => match key {
                    StandardTagKey::TrackTitle => track_metadata.title = tag.value.to_string(),
                    StandardTagKey::TrackNumber => {
                        track_metadata.track_number = tag.value.to_string()
                    }
                    StandardTagKey::Album => track_metadata.album = tag.value.to_string(),
                    StandardTagKey::Artist => track_metadata.artist = vec![tag.value.to_string()],
                    StandardTagKey::Date => {
                        track_metadata.year = (&tag.value.to_string()[..4]).to_string()
                    }
                    StandardTagKey::Comment => track_metadata.comment = tag.value.to_string(),

                    _ => continue,
                },
                None => continue,
            }
        }

        let mut album_art_raw: Box<[u8]> = Box::new([]);

        // tiriamoci fuori il raw album data
        for visual in metadata.visuals().iter() {
            album_art_raw = visual.data.clone();
        }

        if album_art_raw.len() != 0 {
            // se la immagine è presente allora esegui il seguente blocco di codice
            // cerchiamo di capire se la immagine è troppo grande o no, se no,
            // allora track_metadata.album_art ottiene lo stesso, altrimenti, track_metadata.album_art ha una immagine nuova
            // TODO skippa questa sezione se abbiamo già salvato in cache la immagine già processata
            let reader = ImageReader::new(Cursor::new(album_art_raw.clone()))
                .with_guessed_format()
                .expect("Apparently Cursor io never fails?");

            let image = reader.decode().unwrap();
            if image.width() > 500 || image.height() > 500 {
                let new_image = image.resize(500, 500, FilterType::Gaussian);

                let mut buffer = Cursor::new(Vec::new());
                new_image.write_to(&mut buffer, ImageFormat::Jpeg).unwrap();
                track_metadata.album_art = buffer.into_inner().into_boxed_slice();
            } else {
                track_metadata.album_art = album_art_raw;
            }
        } else {
            let parent = self.src_path.parent().unwrap(); // TODO to check, can the parent be just root?

            let mut cover_image_path: Option<PathBuf> = None;

            let search_jpg = parent.to_string_lossy().to_string() + "/*.jpg";
            let search_jpeg = parent.to_string_lossy().to_string() + "/*.jpeg";
            let search_png = parent.to_string_lossy().to_string() + "/*.png";

            //TODO what to do if there is more than one image file??
            // for now it just selects the first one that it finds
            for file in glob(&search_jpg)
                .unwrap()
                .chain(glob(&search_jpeg).unwrap())
                .chain(glob(&search_png).unwrap())
            {
                cover_image_path = Some(file.unwrap());

                break;
            }

            match cover_image_path {
                Some(path) => {
                    let image = ImageReader::open(path).unwrap().decode().unwrap();
                    let mut buffer = Cursor::new(Vec::new());

                    if image.width() > 500 || image.height() > 500 {
                        let new_image = image.resize(500, 500, FilterType::Gaussian);

                        new_image.write_to(&mut buffer, ImageFormat::Jpeg).unwrap();
                        track_metadata.album_art = buffer.into_inner().into_boxed_slice();
                    } else {
                        image.write_to(&mut buffer, ImageFormat::Jpeg).unwrap();
                        track_metadata.album_art = buffer.into_inner().into_boxed_slice();
                    }
                }
                None => track_metadata.album_art = album_art_raw,
            }
        }

        Ok(track_metadata)
    }
    pub fn convert_file_to_mp3(&self, output_path: PathBuf) -> Result<(), Error> {
        let (pcm_data, track_metadata) = self.decode_input().unwrap();

        // TODO maybe allow to export in more formats
        let mp3_bytes;
        match &self.to_type {
            AudioFiletype::MP3 => {
                mp3_bytes = AudioConverter::encode_to_mp3(pcm_data, &track_metadata)
            }
            _ => panic!("not implemented"),
        }

        if self.output_based_on_metadata {
            let second_half_of_path: String = "/".to_string() + &track_metadata.album + "/";
            let dir_path = append_to_path(output_path, &second_half_of_path);

            if !dir_path.exists() {
                if let Err(e) = fs::create_dir_all(&dir_path) {
                    eprintln!("Error creating directory: {}", e);
                } else {
                    println!("Directory created: {}", dir_path.display());
                }
            }

            let filename = format_track_number(&track_metadata.track_number)
                + " - "
                + &track_metadata.title
                + ".mp3";
            let sanitized_filename = filename.replace(":", "_").replace("/", "_");

            let full_path = append_to_path(dir_path, &sanitized_filename);
            let mut file = File::create(full_path).unwrap();
            file.write_all(&mp3_bytes).unwrap();
        } else {
            let mut file = File::create(output_path).unwrap();
            file.write_all(&mp3_bytes).unwrap();
        }
        Ok(())
    }

    fn decode_input(&self) -> Result<(Vec<Vec<f32>>, TrackMetadata), Error> {
        let mut hint = Hint::new();

        match self.from_type {
            AudioFiletype::FLAC => hint.with_extension("flac"),
            AudioFiletype::MP3 => hint.with_extension("mp3"),
            AudioFiletype::OGG => hint.with_extension("ogg"),

        };

        let src = File::open(self.src_path.clone()).expect("failed to open .flac file");
        let mss_src = MediaSourceStream::new(Box::new(src), Default::default());

        let meta_opts: MetadataOptions = Default::default();
        let fmt_opts: FormatOptions = Default::default();

        let mut probed = symphonia::default::get_probe()
            .format(&hint, mss_src, &fmt_opts, &meta_opts)
            .expect("unsupported format");

        let mut format = probed.format;

        let track_metadata_res;
        if let Some(metadata_rev) = format.metadata().current() {
            let binding = format.metadata();
            track_metadata_res = self._extract_metadata(binding)
        } else if let Some(metadata_rev) = probed.metadata.get().as_ref().and_then(|m| m.current())
        {
            let binding = probed.metadata.get().unwrap();
            track_metadata_res = self._extract_metadata(binding)
        } else {
            return Err(Error::Unsupported("no metadata found"));
        }

        let mut track_metadata = track_metadata_res.unwrap();

        ////////////////////////////////////////////////////

        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .expect("no supported audio tracks");

        let params = &track.codec_params;
        let dec_opts: DecoderOptions = Default::default();

        if let Some(sample_rate) = params.sample_rate {
            track_metadata.sample_rate = sample_rate
        } else {
            println!("Sample rate information is not available.");
        }

        let mut decoder = symphonia::default::get_codecs()
            .make(params, &dec_opts)
            .expect("unsupported codec");

        let track_id = track.id;

        let mut pcm_data: Vec<Vec<f32>> = vec![Vec::new(); 2];

        let loop_result = loop {
            let packet = match format.next_packet() {
                Ok(packet) => packet,
                Err(err) => break Err(err),
            };

            while !format.metadata().is_latest() {
                // Pop the old head of the metadata queue.
                format.metadata().pop();
                // Consume the new metadata at the head of the metadata queue.
            }

            if packet.track_id() != track_id {
                continue;
            }

            let decoded = match decoder.decode(&packet) {
                Ok(decoded) => decoded,
                Err(err) => break Err(err),
            };

            match decoded {
                AudioBufferRef::U8(input) => convert_samples(input, &mut pcm_data),
                AudioBufferRef::U16(input) => convert_samples(input, &mut pcm_data),
                AudioBufferRef::U24(input) => convert_samples(input, &mut pcm_data),
                AudioBufferRef::U32(input) => convert_samples(input, &mut pcm_data),
                AudioBufferRef::S8(input) => convert_samples(input, &mut pcm_data),
                AudioBufferRef::S16(input) => convert_samples(input, &mut pcm_data),
                AudioBufferRef::S24(input) => convert_samples(input, &mut pcm_data),
                AudioBufferRef::S32(input) => convert_samples(input, &mut pcm_data),
                AudioBufferRef::F32(input) => convert_samples(input, &mut pcm_data),
                AudioBufferRef::F64(input) => convert_samples(input, &mut pcm_data),
            }
        };

        let _res = self.ignore_end_of_stream_error(loop_result);

        Ok((pcm_data, track_metadata))
    }

    fn encode_to_mp3(pcm_data: Vec<Vec<f32>>, track_metadata: &TrackMetadata) -> Vec<u8> {
        let mut mp3_encoder = Builder::new().expect("Create LAME builder");
        mp3_encoder.set_num_channels(2).expect("set channels");
        mp3_encoder
            .set_sample_rate(track_metadata.sample_rate)
            .expect("set sample rate");
        mp3_encoder
            .set_brate(Bitrate::Kbps192)
            .expect("set bitrate");
        mp3_encoder.set_quality(Quality::Best).expect("set quality");

        let byte_slice: Vec<u8> = track_metadata.artist.concat().into_bytes();

        mp3_encoder
            .set_id3_tag(Id3Tag {
                title: track_metadata.title.as_ref(),
                artist: &byte_slice,
                album: track_metadata.album.as_ref(),
                album_art: &*track_metadata.album_art,
                year: track_metadata.year.as_ref(),
                comment: track_metadata.comment.as_ref(),
            })
            .expect("TODO: panic message");
        let mut mp3_encoder = mp3_encoder.build().expect("To initialize LAME encoder");

        //use actual PCM data
        let input = DualPcm {
            left: &pcm_data[0],
            right: &pcm_data[1],
        };

        let mut mp3_out_buffer = Vec::new();
        mp3_out_buffer.reserve(max_required_buffer_size(input.left.len()));
        let encoded_size = mp3_encoder
            .encode(input, mp3_out_buffer.spare_capacity_mut())
            .expect("To encode");
        unsafe {
            mp3_out_buffer.set_len(mp3_out_buffer.len().wrapping_add(encoded_size));
        }

        let encoded_size = mp3_encoder
            .flush::<FlushNoGap>(mp3_out_buffer.spare_capacity_mut())
            .expect("to flush");
        unsafe {
            mp3_out_buffer.set_len(mp3_out_buffer.len().wrapping_add(encoded_size));
        }

        mp3_out_buffer
    }

    fn ignore_end_of_stream_error(&self, result: Result<(), Error>) -> Result<(), Error> {
        match result {
            Err(Error::IoError(err))
                if err.kind() == std::io::ErrorKind::UnexpectedEof
                    && err.to_string() == "end of stream" =>
            {
                // Do not treat "end of stream" as a fatal error. It's the currently only way a
                // format reader can indicate the media is complete.
                Ok(())
            }
            _ => result,
        }
    }
}

fn format_track_number(str: &str) -> String {
    if str.len() > 1 {
        str.to_string()
    } else {
        "0".to_string() + str
    }
}

fn append_to_path(p: PathBuf, s: &str) -> PathBuf {
    let mut p = p.into_os_string();
    p.push(s);
    p.into()
}

fn convert_samples<S>(input: Cow<AudioBuffer<S>>, output: &mut Vec<Vec<f32>>)
where
    S: Sample + IntoSample<f32>,
{
    for (channel, dest) in output.iter_mut().enumerate() {
        let src = input.chan(channel);
        dest.extend(src.iter().map(|&s| s.into_sample()));
    }
}

#[cfg(test)]
mod tests {
    use crate::app::converter::{AudioConverter, AudioFiletype};
    use std::path::PathBuf;

    #[test]

    fn test_hashing() {
        use std::hash::{DefaultHasher, Hasher};

        let mut hasher = DefaultHasher::new();
        let data = [0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef];

        hasher.write(&data);

        println!("Hash is {:x}!", hasher.finish());
    }

    #[test]

    fn test_mp3() {
        let input_path = PathBuf::from("test_media/test.mp3");
        let dest_path = PathBuf::from("test_media/");
        let audio_converter = AudioConverter::new(input_path.clone(), AudioFiletype::MP3);
        let res = audio_converter.convert_file_to_mp3(dest_path);
    }
}
