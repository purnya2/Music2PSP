use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Instant;

use egui_extras::{StripBuilder, Size};

use rfd::FileDialog;
use std::sync::Arc;
use eframe::{egui_glow, glow};

use egui::{FontData, FontDefinitions, FontFamily, Frame};

use std::default::Default;
use glob::glob;
use crate::app::converter::{AudioConverter, AudioFiletype};
use std::thread;
use eframe::epaint::mutex::Mutex;
use eframe::glow::HasContext;

mod converter;


pub struct TemplateApp {
    // Example stuff:
    label: String,
    folder_directory: PathBuf,
    folder_directories: HashSet<PathBuf>,
    destination_directory: Option<PathBuf>,
    value: f32,
    start_time: Instant,
    xmbwaveshader: Arc<Mutex<XmbWaveShader>>,
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
            label: "Hello World!".to_owned(),
            folder_directory : PathBuf::new(),
            destination_directory: None,
            folder_directories : HashSet::new(),
            value: 2.7,
            start_time: Instant::now(),
            xmbwaveshader: Arc::new(Mutex::new(XmbWaveShader::new(gl)))
        }
    }
}

impl eframe::App for TemplateApp {
    /*/// Called by the framework to save state before shutdown.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }*/



    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.paint_on_window_background(ctx);

        ctx.request_repaint();


        /*egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            // The top panel is often a good place for a menu bar:
            egui::menu::bar(ui, |ui| {
                // NOTE: no File->Quit on web pages!
                let is_web = cfg!(target_arch = "wasm32");
                if !is_web {
                    ui.menu_button("File", |ui| {
                        if ui.button("Quit").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        } else if ui.button("Quit2").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    ui.add_space(16.0);
                }
            });
        });*/




        egui::CentralPanel::default()
            .frame(Frame::none().inner_margin(egui::Margin::same(30.0)))
            .show(ctx, |ui| {

            ui.heading("Music2PSP Converter");


            if ui.button("Add Folder").clicked() {
                if let Some(file_path) = FileDialog::new().pick_folder(){
                    self.folder_directories.insert(file_path);
                }
            }




            if ui.button("Select Destination Folder").clicked() {
                if let Some(file_path) = FileDialog::new().pick_folder(){
                    self.destination_directory = Some(file_path);
                } else {
                    self.destination_directory = None;
                }
            }
            ui.horizontal(|ui| {
                ui.label("dest dir:");
                let dst_ops = self.destination_directory.clone();
                let mut dst_str = "".to_string();
                match dst_ops {
                    Some(dir) => {
                        dst_str = dir.to_string_lossy().to_string()
                    },
                    None => dst_str = "".to_string(),
                }
                ui.text_edit_singleline(&mut dst_str);
            });

            if ui.button("convert folder/s").clicked() {
                let dst_ops = self.destination_directory.clone();
                match dst_ops {
                    Some(dir) => {
                        for folder in &self.folder_directories {
                            convert_files_in_folder(folder, dir.clone());
                        }
                    },
                    None => println!("No destination here man!"),
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
                            table_ui(ui,&mut self.folder_directories);
                        });
                    });

                });

            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                egui::warn_if_debug_build(ui);
            });


        });
    }

}



    impl TemplateApp{

    fn paint_on_window_background(&mut self,ctx: &egui::Context) {
        let screen_rect = ctx.screen_rect();

        let start_time = self.start_time.clone();
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


fn convert_files_in_folder(dir: &PathBuf, destination: PathBuf) {

    let search_path = dir.to_string_lossy().to_string() + "/*.flac";
    for file in glob(&search_path).expect("failz on da glob pattern"){
        let dest = destination.clone(); // TODO how many  clones?

        let _handle = thread::spawn( move ||{
            let input_path = file.unwrap();
            let binding = input_path.clone();
            let filename = binding.file_name().unwrap();
            println!("Currently converting : {:?}", filename);

            let audio_converter = AudioConverter::new(input_path, AudioFiletype::MP3);
            let res = audio_converter.convert_file_to_mp3( dest.clone());

            match res{
                Ok(()) => println!("Converted! : {:?}", filename),
                Err(e) => eprintln!("Error for file {:?}... : {}",filename, e),
            }
        });
    }
}


fn table_ui(ui: &mut egui::Ui, data: &mut HashSet<PathBuf>) {
    use egui_extras::{TableBuilder, Column};

    let mut to_remove = Vec::new();

    let len = ui.available_width()*0.6;
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
            for entry in data.iter(){
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

    pub fn new(gl: &glow::Context) -> Self{
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
                        vec3 col = vec3(0.3+((uv.x+uv.y)/3.0),0.3+((uv.x+uv.y)/3.0),0.3+((uv.x+uv.y)/3.0));

                        col+=clamp(wave(uv,1.0,0.4,10.0,4.0,0.1,1.0,0.5),0.0,0.4);
                        col+=clamp(wave(uv,2.0,0.3,9.0,4.0,0.1,1.0,0.5),0.0,1.0);
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
    fn paint_static(&self, gl: &glow::Context, time_start: Instant) {
        /*
        use glow::HasContext as _;
        unsafe {
            let shader_version = egui_glow::ShaderVersion::get(gl);

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
                        vec3 col = vec3(0.3+((uv.x+uv.y)/3.0),0.3+((uv.x+uv.y)/3.0),0.3+((uv.x+uv.y)/3.0));

                        col+=clamp(wave(uv,1.0,0.4,10.0,4.0,0.1,1.0,0.5),0.0,0.4);
                        col+=clamp(wave(uv,2.0,0.3,9.0,4.0,0.1,1.0,0.5),0.0,0.4);
                        out_color = vec4(col,1.0);

                    }
                "#,
            );

            let program = gl.create_program().expect("Cannot create program");
            let shaders = [
                (glow::VERTEX_SHADER, vertex_shader_source),
                (glow::FRAGMENT_SHADER, fragment_shader_source),
            ];

            let compiled_shaders: Vec<_> = shaders
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
                gl.uniform_1_f32(Some(&i_time_location), time_start.elapsed().as_secs_f32());
            }
            gl.bind_vertex_array(Some(vertex_array));
            gl.draw_arrays(glow::TRIANGLE_FAN, 0, 4);

            // Cleanup
            for shader in compiled_shaders {
                gl.detach_shader(program, shader);
                gl.delete_shader(shader);
            }
            gl.delete_program(program);
            gl.delete_vertex_array(vertex_array);
        }*/

        use glow::HasContext as _;
        unsafe {
            gl.use_program(Some(self.program));
            gl.uniform_1_f32(
                gl.get_uniform_location(self.program, "iTime").as_ref(),
                time_start.elapsed().as_secs_f32(),
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

