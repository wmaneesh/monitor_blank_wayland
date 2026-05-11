use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer,
    delegate_registry, delegate_seat, delegate_shm,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        Capability, SeatHandler, SeatState,
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers},
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
    },
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
    },
    shm::{Shm, ShmHandler, slot::SlotPool},
};
use std::sync::atomic::Ordering;
use std::{collections::HashMap, convert::TryInto, num::NonZeroU32, process::Command};
use wayland_client::{
    Connection, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface},
};

pub fn run_monitor_blank(args: Vec<String>, term: std::sync::Arc<std::sync::atomic::AtomicBool>) {
    env_logger::init();
    //connect to compositor (server)
    let conn = Connection::connect_to_env().unwrap();

    // let args: Vec<String> = env::args().collect();
    // dbg!(args);

    //enumarate the globals and find all the protocals the server implements
    let (globals, mut event_queue) = registry_queue_init(&conn).unwrap();
    let qh = event_queue.handle();

    //grab the compositor (not the server) that allows configuring surfaces to be presented
    let compositor = CompositorState::bind(&globals, &qh).expect("wl_compositor is not available");
    //this is the layer_shell that will be used to create a layer
    let layer_shell = LayerShell::bind(&globals, &qh).expect("layer_shell is not available");
    //uses software to render the buffer instead of a gpu for now
    let shm = Shm::bind(&globals, &qh).expect("wl_shm is not avaiable");
    let pool = SlotPool::new(1920 * 1080 * 4 * 2, &shm).expect("Failed to create pool");

    let mut monitor_configs = Vec::new();

    for arg in args {
        let parts: Vec<&str> = arg.split(':').collect();

        if parts.len() != 3 {
            eprintln!(
                "Invalid monitor config '{}'. Expected OUTPUT:DIM:RESTORE",
                arg
            );
            continue;
        }

        let output = parts[0].to_string();

        let dim = parts[1].parse::<u8>().expect("Invalid dim brightness");

        let restore = parts[2].parse::<u8>().expect("Invalid restore brightness");

        monitor_configs.push(MonitorBrightnessConfig {
            output,
            dim,
            restore,
        });
    }

    //get the display map
    let display_map = get_ddc_display_map();

    let mut simple_layer = SimpleLayer {
        registry_state: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        output_state: OutputState::new(&globals, &qh),
        shm,
        exit: false,
        pool,
        keyboard: None,
        keyboard_focus: false,
        pointer: None,
        compositor,
        layer_shell,
        active_layers: Vec::new(),
        monitor_configs: monitor_configs,
        display_map: display_map,
    };

    simple_layer.set_brightness(false);

    // We don't draw immediately, the configure will notify us when to first draw.
    loop {
        event_queue.blocking_dispatch(&mut simple_layer).unwrap();

        if term.load(Ordering::Relaxed) {
            simple_layer.remove_all_layers();
        }

        if simple_layer.exit {
            println!("exiting example");
            break;
        }
    }
}

fn get_ddc_display_map() -> HashMap<String, u8> {
    let output = Command::new("ddcutil")
        .arg("detect")
        .output()
        .expect("Failed to run ddcutil detect");

    let stdout = String::from_utf8_lossy(&output.stdout);

    let mut map = HashMap::new();
    let mut current_display: Option<u8> = None;

    for line in stdout.lines() {
        let trimmed = line.trim();

        // Example:
        // Display 1
        if trimmed.starts_with("Display ") {
            current_display = trimmed
                .split_whitespace()
                .nth(1)
                .and_then(|s| s.parse::<u8>().ok());
        }

        // Example:
        // DRM_connector: card1-DP-1
        if trimmed.starts_with("DRM_connector:") {
            if let Some(display_id) = current_display {
                if let Some(connector) = trimmed.split_whitespace().last() {
                    // card1-DP-1 -> DP-1
                    let hypr_output = connector.split('-').skip(1).collect::<Vec<_>>().join("-");

                    println!(
                        "Mapped Hyprland output {} -> DDC display {}",
                        hypr_output, display_id
                    );

                    map.insert(hypr_output, display_id);
                }
            }
        }
    }
    map
}

#[derive(Debug, Clone)]
struct MonitorBrightnessConfig {
    output: String,
    dim: u8,
    restore: u8,
}

struct ActiveLayer {
    output: wl_output::WlOutput,
    output_name: Option<String>,
    layer: LayerSurface,
    width: u32,
    height: u32,
    needs_redraw: bool,
}

struct SimpleLayer {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    shm: Shm,
    exit: bool,
    pool: SlotPool,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    keyboard_focus: bool,
    pointer: Option<wl_pointer::WlPointer>,
    compositor: CompositorState,
    layer_shell: LayerShell,
    active_layers: Vec<ActiveLayer>,
    monitor_configs: Vec<MonitorBrightnessConfig>,
    display_map: HashMap<String, u8>,
}

impl SimpleLayer {
    fn create_layer(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
        let info = self.output_state.info(&_output).unwrap();
        let surface = self.compositor.create_surface(_qh);
        let layer = self.layer_shell.create_layer_surface(
            _qh,
            surface,
            Layer::Overlay,
            Some("black_layer"),
            Some(&_output),
        );

        layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer.set_keyboard_interactivity(KeyboardInteractivity::OnDemand);
        layer.set_size(0, 0);
        layer.set_exclusive_zone(-1);

        layer.commit();

        let active_layer = ActiveLayer {
            output: _output.clone(),
            output_name: info.name.clone(),
            layer,
            width: 0,
            height: 0,
            needs_redraw: true,
        };

        self.active_layers.push(active_layer);
        println!("output name {:?}", info.name);
    }

    fn remove_all_layers(&mut self) {
        self.set_brightness(true);

        self.active_layers
            .iter()
            .for_each(|l| print!("Removing layer from output {:?}\n", l.output_name));
        self.active_layers.clear();
        self.exit = true;
    }

    pub fn draw(&mut self, qh: &QueueHandle<Self>) {
        let pool = &mut self.pool;
        for layer in &mut self.active_layers {
            if layer.needs_redraw && layer.width > 0 && layer.height > 0 {
                Self::draw_layer(pool, qh, &layer.layer, layer.width, layer.height);
                layer.needs_redraw = false;
            }
        }
    }

    fn draw_layer(
        pool: &mut SlotPool,
        qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        width: u32,
        height: u32,
    ) {
        let width = width;
        let height = height;
        let stride = width as i32 * 4;

        let (buffer, canvas) = pool
            .create_buffer(
                width as i32,
                height as i32,
                stride,
                wl_shm::Format::Argb8888,
            )
            .expect("create buffer");

        let color: u32 = 0xFF000000; // black

        canvas.chunks_exact_mut(4).for_each(|chunk| {
            let array: &mut [u8; 4] = chunk.try_into().unwrap();
            *array = color.to_le_bytes();
        });

        // // Damage the entire window
        layer
            .wl_surface()
            .damage_buffer(0, 0, width as i32, height as i32);

        // Request our next frame
        layer.wl_surface().frame(qh, layer.wl_surface().clone());

        // Attach and commit to present.
        buffer.attach_to(layer.wl_surface()).expect("buffer attach");
        layer.commit();
    }

    fn set_brightness(&self, restore: bool) {
        for config in &self.monitor_configs {
            let brightness = if restore { config.restore } else { config.dim };

            match self.display_map.get(&config.output) {
                Some(display_id) => {
                    println!("Setting {} brightness to {}", config.output, brightness);

                    let display_id = *display_id;

                    std::thread::spawn(move || {
                        let _ = Command::new("ddcutil")
                            .args([
                                "--display",
                                &display_id.to_string(),
                                "--sleep-multiplier",
                                "0.3",
                                "--noverify",
                                "setvcp",
                                "10",
                                &brightness.to_string(),
                            ])
                            .spawn();
                    });
                }

                None => {
                    eprintln!("No DDC mapping found for {}", config.output);
                }
            }
        }
    }
}

impl OutputHandler for SimpleLayer {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
        if let Some(info) = self.output_state.info(&_output) {
            if let Some(name) = &info.name {
                if self.monitor_configs.iter().any(|c| c.output == *name) {
                    println!("Creating layer on output: {}", name);
                    self.create_layer(_conn, _qh, _output);
                }
            }
        }
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
        let active_layer = self.active_layers.iter().find(|l| l.output == _output);
        print!(
            "Removing layer from output {:?}\n",
            active_layer.unwrap().output_name
        );

        self.active_layers.retain(|l| l.output != _output);
    }
}

impl LayerShellHandler for SimpleLayer {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {
        self.set_brightness(true);
        self.active_layers.clear();
        self.exit = true;
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        if let Some(active) = self.active_layers.iter_mut().find(|p| &p.layer == _layer) {
            if let (Some(w), Some(h)) = (
                NonZeroU32::new(configure.new_size.0),
                NonZeroU32::new(configure.new_size.1),
            ) {
                active.width = w.get();
                active.height = h.get();
                active.needs_redraw = true;
            }
        }

        self.draw(qh);
    }
}

impl KeyboardHandler for SimpleLayer {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        keysyms: &[Keysym],
    ) {
        if self
            .active_layers
            .iter()
            .any(|l| l.layer.wl_surface() == surface)
        {
            println!("Keyboard focus on window with pressed syms: {keysyms:?}");
            self.keyboard_focus = true;
        }
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        _: u32,
    ) {
        if self
            .active_layers
            .iter()
            .any(|l| l.layer.wl_surface() == surface)
        {
            println!("Release keyboard focus on window");
            self.keyboard_focus = false;
        }
    }

    fn press_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        println!("Key press: {event:?}");
        // press 'esc' to close layer
        if event.keysym == Keysym::Escape {
            self.remove_all_layers();
        }
    }

    fn repeat_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        event: KeyEvent,
    ) {
        println!("Key repeat: {event:?}");
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        println!("Key release: {event:?}");
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _serial: u32,
        modifiers: Modifiers,
        _raw_modifiers: RawModifiers,
        _layout: u32,
    ) {
        println!("Update modifiers: {modifiers:?}");
    }
}

impl PointerHandler for SimpleLayer {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        use PointerEventKind::*;
        for event in events {
            // Ignore events for other surfaces
            if self
                .active_layers
                .iter()
                .all(|l| l.layer.wl_surface() != &event.surface)
            {
                continue;
            }

            match event.kind {
                Enter { .. } => {
                    println!("Pointer entered @{:?}", event.position);
                    let cursor_surface = self.compositor.create_surface(_qh);
                    _pointer.set_cursor(0, Some(&cursor_surface), 0, 0);
                }
                Leave { .. } => {
                    println!("Pointer left");
                    _pointer.set_cursor(0, None, 0, 0);
                }
                Motion { .. } => {
                    println!("Motions {:?}", event.position);
                }
                Press { button, .. } => {
                    println!("Press {:x} @ {:?}", button, event.position);
                }
                Release { button, .. } => {
                    println!("Release {:x} @ {:?}", button, event.position);
                }
                Axis {
                    horizontal,
                    vertical,
                    ..
                } => {
                    println!("Scroll H:{horizontal:?}, V:{vertical:?}");
                }
            }
        }
    }
}

impl CompositorHandler for SimpleLayer {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_factor: i32,
    ) {
        // Not needed for this example.
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
        // Not needed for this example.
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
        // self.draw(qh);
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
        // Not needed for this example.
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
        // Not needed for this example.
    }
}

impl ShmHandler for SimpleLayer {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl SeatHandler for SimpleLayer {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            println!("Set keyboard capability");
            let keyboard = self
                .seat_state
                .get_keyboard(qh, &seat, None)
                .expect("Failed to create keyboard");
            self.keyboard = Some(keyboard);
        }

        if capability == Capability::Pointer && self.pointer.is_none() {
            println!("Set pointer capability");
            let pointer = self
                .seat_state
                .get_pointer(qh, &seat)
                .expect("Failed to create pointer");
            self.pointer = Some(pointer);
        }
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard && self.keyboard.is_some() {
            println!("Unset keyboard capability");
            self.keyboard.take().unwrap().release();
        }

        if capability == Capability::Pointer && self.pointer.is_some() {
            println!("Unset pointer capability");
            self.pointer.take().unwrap().release();
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

delegate_compositor!(SimpleLayer);
delegate_output!(SimpleLayer);
delegate_shm!(SimpleLayer);

delegate_seat!(SimpleLayer);
delegate_keyboard!(SimpleLayer);
delegate_pointer!(SimpleLayer);

delegate_layer!(SimpleLayer);

delegate_registry!(SimpleLayer);

impl ProvidesRegistryState for SimpleLayer {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}
