use crate::app::converter::{AudioConverter, AudioFiletype};
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

pub struct ThreadHandler {
    pub num_processing: Arc<AtomicUsize>,
    pub num_finished: Arc<AtomicUsize>,

    file_buffer: Vec<PathBuf>,
    pub destination: PathBuf,
    pub is_busy: Arc<AtomicBool>,
}

impl ThreadHandler {
    pub fn new() -> Self {
        Self {
            num_processing: Arc::new(AtomicUsize::new(0)),
            num_finished: Arc::new(AtomicUsize::new(0)),
            file_buffer: Vec::new(),
            destination: PathBuf::new(),
            is_busy: Arc::new(AtomicBool::new(false)),
        }
    }

    fn process( input_path: PathBuf, dest_path: PathBuf) {
        let binding = input_path.clone();
        let filename = binding.file_name().unwrap();
        println!("Currently converting : {:?}", filename);
        let audio_converter = AudioConverter::new(input_path, AudioFiletype::MP3);
        let res = audio_converter.convert_file_to_mp3(dest_path);

        match res {
            Ok(()) => println!("Converted! : {:?}", filename),
            Err(e) => eprintln!("Error for file {:?}... : {}", filename, e),
        }
    }
    // TODO : siamo sicuri che la cosa migliore da fare è .clone di pathbuf?
    pub fn execute_threads(&self) {
        let num_processing = Arc::clone(&self.num_processing);
        let num_finished = Arc::clone(&self.num_finished);
        let is_busy = Arc::clone(&self.is_busy);

        let file_buffer = self.file_buffer.clone();
        let destination = self.destination.clone();

        let handle = thread::spawn(move || {
            is_busy.store(true, Ordering::Relaxed);
            file_buffer.par_iter().for_each(|input| {
                num_processing.fetch_add(1, Ordering::SeqCst);
                ThreadHandler::process(input.clone(), destination.clone());
                num_finished.fetch_add(1, Ordering::SeqCst);
            });
            is_busy.store(false, Ordering::Relaxed);


        });

    }

    pub fn add_files(&mut self, files: Vec<PathBuf>) {
        self.file_buffer.extend(files);
    }
}
