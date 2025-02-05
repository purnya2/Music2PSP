use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Instant;

use egui_extras::{Size, StripBuilder};

use eframe::{egui_glow, glow};
use rfd::FileDialog;
use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily, Frame};

use crate::app::converter::{AudioConverter, AudioFiletype};
use crate::app::thread_handler::ThreadHandler;
use glob::glob;
use std::default::Default;
use std::fs::DirEntry;
use std::sync::atomic::Ordering;

use eframe::epaint::mutex::Mutex;
use std::thread;

mod converter;
mod thread_handler;

pub struct TemplateApp {
    // Example stuff:
    folder_directories: HashSet<PathBuf>,
    destination_directory: Option<PathBuf>,
    start_time: Instant,
    xmbwaveshader: Arc<Mutex<XmbWaveShader>>,
    thread_handler: ThreadHandler,
    t: f32,
    acc: f32,
}

impl TemplateApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // This is also where you can customize the look and feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.
        let mut fonts = FontDefinitions::default();
        fonts.font_data.insert(
            "my_font".to_owned(), // Name of the font
            FontData::from_static(include_bytes!("../assets/fonts/FOT-NewRodinProDB.otf")), // Load the font
        );

        fonts
            .families
            .get_mut(&FontFamily::Proportional)
            .unwrap()
            .insert(0, "my_font".to_owned());
        cc.egui_ctx.set_fonts(fonts);

        let gl = cc
            .gl
            .as_ref()
            .expect("You need to run eframe with the glow backend");

        Self {
            destination_directory: None,
            folder_directories: HashSet::new(),
            start_time: Instant::now(),
            xmbwaveshader: Arc::new(Mutex::new(XmbWaveShader::new(gl))),
            thread_handler: ThreadHandler::new(),
            t: 0.0,
            acc: 0.5,
        }
    }
}

impl eframe::App for TemplateApp {
    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let is_busy = self.thread_handler.is_busy.load(Ordering::Relaxed);
        self.paint_on_window_background(ctx, &is_busy);

        ctx.request_repaint();

        egui::CentralPanel::default()
            .frame(Frame::none().inner_margin(egui::Margin::same(30.0)))
            .show(ctx, |ui| {
                ui.heading("Music2PSP Converter");

                ui.heading(format!(
                    "{}/{}",
                    self.thread_handler
                        .num_finished
                        .load(Ordering::Relaxed)
                        .to_string(),
                    self.thread_handler
                        .num_processing
                        .load(Ordering::Relaxed)
                        .to_string()
                ));

                if ui.button("Add Folder").clicked() {
                    if let Some(file_path) = FileDialog::new().pick_folder() {
                        self.folder_directories.insert(file_path);
                    }
                }

                if ui.button("Select Destination Folder").clicked() {
                    if let Some(file_path) = FileDialog::new().pick_folder() {
                        self.destination_directory = Some(file_path);
                        self.thread_handler.destination =
                            self.destination_directory.clone().unwrap();
                    } else {
                        self.destination_directory = None;
                    }
                }
                ui.horizontal(|ui| {
                    ui.label("dest dir:");
                    let dst_ops = self.destination_directory.clone();
                    let mut dst_str;
                    match dst_ops {
                        Some(dir) => dst_str = dir.to_string_lossy().to_string(),
                        None => dst_str = "".to_string(),
                    }
                    ui.text_edit_singleline(&mut dst_str);
                });

                if ui.button("convert folder/s").clicked() {
                    if !is_busy {
                        let dst_ops = self.destination_directory.clone();
                        match dst_ops {
                            Some(dir) => {
                                for folder in &self.folder_directories {
                                    self.thread_handler
                                        .add_files(collect_files_in_folder(folder));
                                }
                                self.thread_handler.execute_threads();
                            }
                            None => println!("You forgot to put the destination man!"),
                        }
                    }
                }

                ui.separator();
                StripBuilder::new(ui)
                    .size(Size::remainder().at_least(100.0)) // top cell
                    .size(Size::exact(40.0)) // bottom cell
                    .vertical(|mut strip| {
                        // Add the top 'cell'
                        strip.cell(|ui| {
                            egui::ScrollArea::horizontal().show(ui, |ui| {
                                table_ui(ui, &mut self.folder_directories);
                            });
                        });
                    });

                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    egui::warn_if_debug_build(ui);
                });
            });
    }

    fn on_exit(&mut self, gl: Option<&glow::Context>) {
        if let Some(gl) = gl {
            self.xmbwaveshader.lock().destroy(gl);
        }
    }
}
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
impl TemplateApp {
    fn paint_on_window_background(&mut self, ctx: &egui::Context, is_busy: &bool) {
        let screen_rect = ctx.screen_rect();
        let mut start_time: f32 = 0.0;
        if *is_busy {
            self.acc = lerp(self.acc, 0.25, 0.005);
            self.t += self.acc;
            start_time = self.t;
        } else {
            self.acc = lerp(self.acc, 0.01, 0.01);
            self.t += self.acc;
            start_time = self.t;
        }

        let xmbwaveshader = self.xmbwaveshader.clone();

        ctx.layer_painter(egui::LayerId::background())
            .add(egui::PaintCallback {
                rect: screen_rect,
                callback: Arc::new(egui_glow::CallbackFn::new(move |_info, painter| {
                    xmbwaveshader.lock().paint_static(painter.gl(), start_time);
                })),
            });
    }
}

// Collects all the files that have an acceptable extension
fn collect_files_in_folder(dir: &PathBuf) -> Vec<PathBuf> {
    let valid_extensions = ["flac", "ogg", "mp3", "aac"];

    let files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .filter(|entry_res| match entry_res {
            Ok(entry) => match entry.path().extension() {
                Some(str) => valid_extensions.contains(&str.to_str().unwrap()),
                None => false,
            },
            Err(_) => {
                panic!("Error at reading entries!")
            }
        })
        .map(|entry_res| entry_res.unwrap().path())
        .collect();

    files
}

fn table_ui(ui: &mut egui::Ui, data: &mut HashSet<PathBuf>) {
    use egui_extras::{Column, TableBuilder};

    let mut to_remove = Vec::new();

    let len = ui.available_width() * 0.6;
    TableBuilder::new(ui)
        .column(Column::exact(len))
        .column(Column::remainder())
        .header(20.0, |mut header| {
            header.col(|ui| {
                ui.heading("Directory");
            });
            header.col(|ui| {
                ui.heading("Modify");
            });
        })
        .body(|mut body| {
            for entry in data.iter() {
                body.row(15.0, |mut row| {
                    row.col(|ui| {
                        ui.label(entry.to_string_lossy());
                    });
                    row.col(|ui| {
                        if ui.button("remove").clicked() {
                            to_remove.push(entry.clone())
                        }
                    });
                });
            }
        });

    for entry in to_remove {
        data.remove(&entry);
    }
}

struct XmbWaveShader {
    program: glow::Program,
    vertex_array: glow::VertexArray,
}
#[allow(unsafe_code)] // we need unsafe code to use glow
impl XmbWaveShader {
    pub fn new(gl: &glow::Context) -> Self {
        use glow::HasContext as _;
        unsafe {
            let shader_version = egui_glow::ShaderVersion::get(gl);
            let program = gl.create_program().expect("Cannot create program");
            //TODO how the fuck do I put in the iResolution

            // Vertex and fragment shaders
            let (vertex_shader_source, fragment_shader_source) = (
                r#"
                    const vec2 verts[4] = vec2[4](
                    vec2(-1.0, 1.0),  // Top-left
                    vec2(1.0, 1.0),   // Top-right
                    vec2(1.0, -1.0),  // Bottom-right
                    vec2(-1.0, -1.0)  // Bottom-left
                );
                const vec4 colors[4] = vec4[4](
                    vec4(1.0, 0.0, 0.0, 1.0),  // Red
                    vec4(0.0, 1.0, 0.0, 1.0),  // Green
                    vec4(0.0, 0.0, 1.0, 1.0),  // Blue
                    vec4(1.0, 1.0, 0.0, 1.0)   // Yellow
                );
                    out vec4 v_color;
                    void main() {
                        v_color = colors[gl_VertexID];
                        gl_Position = vec4(verts[gl_VertexID], 0.0, 1.0);
                    }
                "#,
                r#"
                    uniform vec2 iResolution;
                    precision mediump float;
                    uniform float iTime;
                    float wave(vec2 uv, float inv_size, float period, float inv_amplitude, float x_offset, float y_offset, float flip, float speed){
                        float wv = ((uv.y-y_offset)*inv_size)-(sin(((x_offset+uv.x)/period)+(speed*iTime))/inv_amplitude);
                        if(abs(wv)>0.4){
                            wv=0.0;
                        }
                        return flip*wv;
                    }



                    in vec4 v_color;
                    out vec4 out_color;
                    void main() {
                        vec2 uv = gl_FragCoord.xy/1000.0;
                        vec3 col = vec3(0.2+((uv.x+uv.y)/3.0),0.2+((uv.x+uv.y)/3.0),0.2+((uv.x+uv.y)/3.0));

                        col+=clamp(wave(uv,1.0,0.4,10.0,4.0,0.1,1.0,0.5),0.0,0.4);
                        col+=clamp(wave(uv,2.0,0.3,9.0,4.0,0.1,1.0,1.0),0.0,0.4);
                        out_color = vec4(col,1.0);

                    }
                "#,
            );

            let shaders = [
                (glow::VERTEX_SHADER, vertex_shader_source),
                (glow::FRAGMENT_SHADER, fragment_shader_source),
            ];

            let _compiled_shaders: Vec<_> = shaders
                .iter()
                .map(|(shader_type, shader_source)| {
                    let shader = gl.create_shader(*shader_type).unwrap();
                    gl.shader_source(
                        shader,
                        &format!(
                            "{}\n{}",
                            shader_version.version_declaration(),
                            shader_source
                        ),
                    );
                    gl.compile_shader(shader);
                    assert!(
                        gl.get_shader_compile_status(shader),
                        "Failed to compile {shader_type}: {}",
                        gl.get_shader_info_log(shader)
                    );
                    gl.attach_shader(program, shader);
                    shader
                })
                .collect();

            gl.link_program(program);
            assert!(
                gl.get_program_link_status(program),
                "Program link failed: {}",
                gl.get_program_info_log(program)
            );

            let vertex_array = gl.create_vertex_array().unwrap();

            gl.use_program(Some(program));
            if let Some(i_time_location) = gl.get_uniform_location(program, "iTime") {
                gl.uniform_1_f32(Some(&i_time_location), 0f32);
            }
            gl.bind_vertex_array(Some(vertex_array));
            gl.draw_arrays(glow::TRIANGLE_FAN, 0, 4);

            Self {
                program,
                vertex_array,
            }
        }
    }
    fn paint_static(&self, gl: &glow::Context, time_start: f32) {
        use glow::HasContext as _;
        unsafe {
            gl.use_program(Some(self.program));
            gl.uniform_1_f32(
                gl.get_uniform_location(self.program, "iTime").as_ref(),
                time_start,
            );
            gl.bind_vertex_array(Some(self.vertex_array));
            gl.draw_arrays(glow::TRIANGLE_FAN, 0, 4);
        }
    }

    fn destroy(&self, gl: &glow::Context) {
        use glow::HasContext as _;
        unsafe {
            gl.delete_program(self.program);
            gl.delete_vertex_array(self.vertex_array);
        }
    }
}
