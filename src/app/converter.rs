use std::path::PathBuf;
use mp3lame_encoder::*;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::{Metadata, MetadataOptions, StandardTagKey};
use symphonia::core::probe::Hint;
use std::default::Default;
use std::{fmt, fs, thread};
use std::fmt::Formatter;
use std::fs::File;
use std::io::{Cursor, Write};
use glob::glob;
use image::imageops::FilterType;
use image::{ ImageFormat, ImageReader};
use symphonia::core::audio::{AudioBufferRef, Signal};

// TODO a hashset thingy maybe that will store the images
// so that I don't have to regenerate the images continuously


pub enum AudioFiletype {
    MP3,
    FLAC,
}
pub struct AudioConverter {
    from_type : AudioFiletype,
    to_type : AudioFiletype,
    src_path : PathBuf,
    output_based_on_metadata : bool,
}

struct TrackMetadata{
    title: String,
    artist: Vec<String>,
    album: String,
    album_art: Box<[u8]>,
    year: String,
    comment: String,
}

impl fmt::Debug for TrackMetadata{
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
    pub(crate) fn new(src_path : PathBuf, to_type : AudioFiletype) -> Self {

        let from_type = src_path
            .extension()
            .and_then(|ext| ext.to_str())
            .and_then(|ext_str| match ext_str{
                "flac" => Some(AudioFiletype::FLAC),
                "mp3" => Some(AudioFiletype::MP3),
                _ => None,

            }).unwrap_or_else(|| panic!("woah owah no file extension for {:?}", src_path));

        AudioConverter {
            src_path,
            from_type,
            to_type,
            output_based_on_metadata: true
        }
    }



    fn extract_metadata(&self, input_path : PathBuf) -> Result<TrackMetadata,Error>{
        let mut hint = Hint::new();

        if let Some(extension) = input_path.extension() {
            if let Some(extension_str) = extension.to_str(){
                hint.with_extension(extension_str);
            }
        }
        let src = File::open(input_path).expect("failed to open .flac file");

        let mss_src = MediaSourceStream::new(Box::new(src), Default::default());

        let meta_opts : MetadataOptions = Default::default();
        let fmt_opts : FormatOptions = Default::default();

        let probed = symphonia::default::get_probe()
            .format(&hint, mss_src, &fmt_opts, &meta_opts)
            .expect("unsupported format");

        let mut format = probed.format;

        let binding = format.metadata();

        self._extract_metadata(binding)
    }

    fn _extract_metadata(&self,binding: Metadata<'_>) -> Result<TrackMetadata,Error>{

        let metadata = binding.current().unwrap();

        let mut track_metadata : TrackMetadata = TrackMetadata {
            title: "".to_string(),
            artist: vec![],
            album: "".to_string(),
            album_art: Box::new([]),
            year: "".to_string(),
            comment: "".to_string(),
        };

        for tag in metadata.tags().iter() {
            match tag.std_key {
                Some(key) =>{
                    match key {
                        StandardTagKey::TrackTitle => track_metadata.title = tag.value.to_string(),
                        StandardTagKey::Album => track_metadata.album = tag.value.to_string(),
                        StandardTagKey::Artist => track_metadata.artist = vec![tag.value.to_string()],
                        StandardTagKey::Date => track_metadata.year = (&tag.value.to_string()[..4]).to_string(),
                        StandardTagKey::Comment => track_metadata.comment = tag.value.to_string(),

                        _ => continue
                    }
                }
                None => continue,
            }
        }

        let mut album_art_raw: Box<[u8]> = Box::new([]);

        // tiriamoci fuori il raw data
        for visual in metadata.visuals().iter(){
            album_art_raw = visual.data.clone();
        }

        if album_art_raw.len() != 0{ // se la immagine è presente allora esegui il seguente blocco di codice
            // cerchiamo di capire se la immagine è troppo grande o no, se no, allora track_metadata.album_art ottiene lo stesso, altrimenti, track_metadata.album_art ha una immagine
            // nuova
            // TODO skippa questa sezione se abbiamo già salvato in cache la immagine già processata
            let mut reader = ImageReader::new(Cursor::new(album_art_raw.clone()))
                .with_guessed_format()
                .expect("Apparently Cursor io never fails?");

            let image = reader.decode().unwrap();
            if image.width() > 500 || image.height()>500 {
                let new_image = image.resize(500,500,FilterType::Gaussian);

                let mut buffer = Cursor::new(Vec::new());
                new_image.write_to(&mut buffer, ImageFormat::Jpeg).unwrap();
                track_metadata.album_art = buffer.into_inner().into_boxed_slice();


            } else{
                track_metadata.album_art = album_art_raw;
            }
        } else{

            let parent = self.src_path.parent().unwrap(); // TODO to check, can the parent be just root?

            let mut cover_image_path: Option<PathBuf> = None;

            let search_jpg = parent.to_string_lossy().to_string() + "/*.jpg";
            let search_jpeg = parent.to_string_lossy().to_string() + "/*.jpeg";
            let search_png = parent.to_string_lossy().to_string() + "/*.png";

            //TODO what to do if there is more than one image file??
            // for now it just selects the first one that it finds
            for file in glob(&search_jpg).unwrap()
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

                    if image.width() > 500 || image.height()>500 {
                        let new_image = image.resize(500,500,FilterType::Gaussian);

                        new_image.write_to(&mut buffer, ImageFormat::Jpeg).unwrap();
                        track_metadata.album_art = buffer.into_inner().into_boxed_slice();


                    } else{
                        image.write_to(&mut buffer, ImageFormat::Jpeg).unwrap();
                        track_metadata.album_art = buffer.into_inner().into_boxed_slice();
                    }

                },
                None => track_metadata.album_art = album_art_raw,
            }




        }


        Ok(track_metadata)
    }
    pub fn convert_file_to_mp3(&self, output_path: PathBuf) -> Result<(),Error>{

        let (pcm_data_left_vec,pcm_data_right_vec,track_metadata) = self.decode_input().unwrap();
        let mp3_bytes = self.encode_to_mp3(pcm_data_left_vec,pcm_data_right_vec, &track_metadata);

        if self.output_based_on_metadata {

            let second_half_of_path : String = "/".to_string() + &track_metadata.album + "/" ;
            let dir_path = append_to_path(output_path,&second_half_of_path);

            if !dir_path.exists(){
                if let Err(e) = fs::create_dir_all(&dir_path){
                    eprintln!("Error creating directory: {}", e);
                } else {
                    println!("Directory created: {}", dir_path.display());
                }
            }

            let filename = track_metadata.title.clone() + ".mp3";
            let sanitized_filename = filename.replace(":", "_");  // Replace colon with underscore or another valid character

            let full_path = append_to_path(dir_path, &sanitized_filename);
            let mut file = File::create(full_path).unwrap();
            file.write_all(&mp3_bytes).unwrap();
        } else {
            let mut file = File::create(output_path).unwrap();
            file.write_all(&mp3_bytes).unwrap();
        }
        Ok(())
    }

    fn decode_input(&self) ->Result<( Vec<i32>,Vec<i32>,TrackMetadata),Error>{
        let mut hint = Hint::new();

        match self.from_type{
            AudioFiletype::FLAC => hint.with_extension("flac"),
            AudioFiletype::MP3 => hint.with_extension("mp3"),
        };

        let src = File::open(self.src_path.clone()).expect("failed to open .flac file");

        let mss_src = MediaSourceStream::new(Box::new(src), Default::default());

        let meta_opts : MetadataOptions = Default::default();
        let fmt_opts : FormatOptions = Default::default();

        let probed = symphonia::default::get_probe()
            .format(&hint, mss_src, &fmt_opts, &meta_opts)
            .expect("unsupported format");

        let mut format = probed.format;

        let binding = format.metadata();


        let track_metadata = self._extract_metadata(binding).unwrap();

        ////////////////////////////////////////////////////

        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .expect("no supported audio tracks");

        let dec_opts: DecoderOptions = Default::default();

        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &dec_opts)
            .expect("unsupported codec");

        let track_id = track.id;

        let mut pcm_data_left_vec : Vec<i32> = Vec::new();
        let mut pcm_data_right_vec : Vec<i32> = Vec::new();

        let result = loop{
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

            let decoded = match decoder.decode(&packet){
                Ok(decoded) => decoded,
                Err(err) => break Err(err),
            };

            /*match decoded {
                AudioBufferRef::U8(_) => println!("It is a U8 variant."),
                AudioBufferRef::U16(_) => println!("It is a U16 variant."),
                AudioBufferRef::U24(_) => println!("It is a U24 variant."),
                AudioBufferRef::U32(_) => println!("It is a U32 variant."),
                AudioBufferRef::S8(_) => println!("It is a S8 variant."),
                AudioBufferRef::S16(_) => println!("It is a S16 variant."),
                AudioBufferRef::S24(_) => println!("It is a S24 variant."),
                AudioBufferRef::S32(_) => println!("It is a S32 variant."),
                AudioBufferRef::F32(_) => println!("It is a F32 variant."),
                AudioBufferRef::F64(_) => println!("It is a F64 variant."),
            }*/

            match decoded {
                AudioBufferRef::S32(buf) => {

                    let left_channel: Vec<_> = buf.chan(0).to_vec();
                    let right_channel: Vec<_> = buf.chan(1).to_vec();

                    let mut pcm_data_left_vec_t = vec![];
                    let mut pcm_data_right_vec_t = vec![];

                    let handle_left = thread::spawn(move || {
                        for &sample in &left_channel {
                            pcm_data_left_vec_t.push(sample);
                        }
                        pcm_data_left_vec_t
                    });
                    let handle_right= thread::spawn(move || {
                        for &sample in &right_channel {
                            pcm_data_right_vec_t.push(sample);
                        }
                        pcm_data_right_vec_t
                    });

                    pcm_data_left_vec.append(&mut handle_left.join().unwrap());
                    pcm_data_right_vec.append(&mut handle_right.join().unwrap());


                }
                _ => {
                    // Handle other sample formats.
                    unimplemented!()
                }
            }


        };

        let _res = self.ignore_end_of_stream_error(result);


        Ok((pcm_data_left_vec,pcm_data_right_vec,track_metadata))


    }

    fn encode_to_mp3(&self, pcm_left : Vec<i32>,pcm_right : Vec<i32>, track_metadata: &TrackMetadata) -> Vec<u8>{
        let mut mp3_encoder = Builder::new().expect("Create LAME builder");
        mp3_encoder.set_num_channels(2).expect("set channels");
        mp3_encoder.set_sample_rate(44_100).expect("set sample rate");
        mp3_encoder.set_brate(Bitrate::Kbps192).expect("set brate");
        mp3_encoder.set_quality(Quality::Best).expect("set quality");


        let byte_slice: Vec<u8> = track_metadata.artist.concat().into_bytes();


        mp3_encoder.set_id3_tag(Id3Tag {
            title: track_metadata.title.as_ref(),
            artist: &byte_slice,
            album: track_metadata.album.as_ref(),
            album_art: &*track_metadata.album_art,
            year: track_metadata.year.as_ref(),
            comment: track_metadata.comment.as_ref(),
        }).expect("TODO: panic message");
        let mut mp3_encoder = mp3_encoder.build().expect("To initialize LAME encoder");

        //use actual PCM data
        let input = DualPcm {
            left: &pcm_left,
            right: &pcm_right,
        };

        let mut mp3_out_buffer = Vec::new();
        mp3_out_buffer.reserve(max_required_buffer_size(input.left.len()));
        let encoded_size = mp3_encoder.encode(input, mp3_out_buffer.spare_capacity_mut()).expect("To encode");
        unsafe {
            mp3_out_buffer.set_len(mp3_out_buffer.len().wrapping_add(encoded_size));
        }

        let encoded_size = mp3_encoder.flush::<FlushNoGap>(mp3_out_buffer.spare_capacity_mut()).expect("to flush");
        unsafe {
            mp3_out_buffer.set_len(mp3_out_buffer.len().wrapping_add(encoded_size));
        }

        mp3_out_buffer

    }

    pub fn some_function(&self) ->Result<(),()>  {
        Ok(())
    }

    fn ignore_end_of_stream_error(&self, result: Result<(), Error>) -> Result<(),Error> {
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

fn append_to_path(p: PathBuf, s: &str) -> PathBuf {
    let mut p = p.into_os_string();
    p.push(s);
    p.into()
}



#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use image::ImageReader;
    use crate::app::converter::AudioFiletype::MP3;
    use super::*;


    #[test]
    fn test_decode_flac() {
        let something = AudioConverter::new(PathBuf::from("test_media/image_testing/no_file.flac"),MP3);
        assert!(something.convert_file_to_mp3(PathBuf::from("test_media/")).is_ok());
    }

    #[test]

    fn test_metadata_extraction(){
        let audio_converter = AudioConverter::new(PathBuf::from("test_media/no_file.flac"), MP3);

        let metadata = audio_converter.extract_metadata(PathBuf::from("test_media/no_file.flac"));

        match metadata {
            Ok(track_metadata) => {
                println!("{:?}",track_metadata);

                let album_art_raw = track_metadata.album_art;
                println!("{:02X} {:02X} {:02X}", album_art_raw[0],album_art_raw[1],album_art_raw[2]);


                let mut reader = ImageReader::new(Cursor::new(album_art_raw))
                    .with_guessed_format()
                    .expect("Apparently Cursor io never fails?");

                let image = reader.decode().unwrap();

                println!("{:?}, {:?}", image.width(),image.height());
                assert!(true)
            }
            Err(err) => {
                eprintln!("{}",err);
                assert!(false)
            }
        }
    }
}