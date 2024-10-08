#![allow(
    dead_code,
    unused_variables,
    clippy::too_many_arguments,
    clippy::unnecessary_wraps
)]

use anyhow::{anyhow, Result};
use thiserror::Error;

use log::*;

use vk::QueueFamilyProperties;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::keyboard::*;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{self, Window};

use vulkanalia::loader::{LibloadingLoader, LIBRARY};
use vulkanalia::prelude::v1_0::*;
use vulkanalia::Version;
use vulkanalia::vk::ExtDebugUtilsExtension;
use vulkanalia::window as vk_window;

use std::collections::HashSet;
use std::ffi::CStr;
use std::os::raw::c_void;

const PORTABILITY_MACOS_VERSION: Version = Version::new(1,3,216);
const VALIDATION_ENABLED: bool = cfg!(debug_assertions);
const VALIDATION_LAYER: vk::ExtensionName = vk::ExtensionName::from_bytes(b"VK_LAYER_KHRONOS_validation");

#[derive(Default ,Debug)]
struct App {
    window: Option<Window>,
    app: Option<VulkanApp>
}

#[derive(Clone, Debug)]
struct VulkanApp {
    entry: Entry,
    instance: Instance,
    data: AppData,
    device: Device,
}

impl VulkanApp {
    fn create(window: &Window) -> Result<Self> {

        let loader: LibloadingLoader;
        unsafe {
            loader = LibloadingLoader::new(LIBRARY)?;
        }

        let entry: Entry;
        unsafe {
            entry = Entry::new(loader).map_err(|b| anyhow!("{}", b))?;
        }
        let mut data = AppData::default();
        let instance = create_instance(window, &entry, &mut data)?;
        let device = create_logical_decice(&entry, &instance, &mut data)?;

        return Ok(Self {entry, instance, data, device})
    }

    unsafe fn render(&mut self, window: &Window) -> Result<()> {
        return Ok(());
    }

    unsafe fn destroy(&mut self) {
        if VALIDATION_ENABLED {
            self.instance.destroy_debug_utils_messenger_ext(self.data.messenger, None);
        }

        self.device.destroy_device(None);
        self.instance.destroy_instance(None);
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window_props = Window::default_attributes()
        .with_title("Vulkan Testin (Rust)")
        .with_inner_size(LogicalSize::new(1280,720));

        self.window = Some(event_loop.create_window(window_props).unwrap());
        self.app = Some(VulkanApp::create(self.window.as_ref().unwrap()).unwrap());
    }

    fn window_event(
            &mut self,
            event_loop: &ActiveEventLoop,
            window_id: window::WindowId,
            event: WindowEvent,
        ) {
        match event {
            WindowEvent::KeyboardInput { device_id, event, is_synthetic } => {
                match event.logical_key {
                    Key::Named(NamedKey::Escape) => {
                        event_loop.exit();
                    }
                    _ => ()
                }
            }

            WindowEvent::CloseRequested => {
                event_loop.exit();
            }

            _ => ()
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // request redraw when other events have passed
        self.window.as_mut().unwrap().request_redraw();
    }

    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        if self.app.is_some() {
            unsafe {
                self.app.as_mut().unwrap().destroy();
            }
        }
    }

}

#[derive(Clone, Debug, Default)]
struct AppData {
    messenger: vk::DebugUtilsMessengerEXT,
    physical_device: vk::PhysicalDevice,
    graphics_queue: vk::Queue
}

#[derive(Debug, Error)]
#[error("Missing {0}.")]
pub struct SuitabilityError(pub &'static str);

struct QueueFamilyIndices {
    graphics: u32
}

impl QueueFamilyIndices {
    fn get(_instance: &Instance, _data: &AppData, _p_device: vk::PhysicalDevice) -> Result<Self> {
        let properties: Vec<QueueFamilyProperties>;
        let graphics: Option<u32>;

        unsafe {
            properties = _instance.get_physical_device_queue_family_properties(_p_device);

            graphics = properties
            .iter()
            .position(|p| p.queue_flags.contains(vk::QueueFlags::GRAPHICS))
            .map(|i| i as u32);

        }

        if let Some(graphics) = graphics {
            return Ok(Self { graphics});
        } else {
            return Err(anyhow!(SuitabilityError("Missing required queue families.")))
        }

    }
}

unsafe fn pick_physical_device(_instance: &Instance, _data: &mut AppData) -> Result<()> {

    for device in _instance.enumerate_physical_devices()? {
        let properties = _instance.get_physical_device_properties(device);

        if let Err(error) = check_physical_device(_instance, _data, device) {
            warn!("Skipping physical device ('{}'): {}", properties.device_name, error);
        } else {
            info!("Selected physical device ('{}').", properties.device_name);
            _data.physical_device = device;
            return Ok(());
        }
    }

    return Err(anyhow!("Failed to find suitable physical device."));

}

unsafe fn check_physical_device(_instance: &Instance, _data: &AppData, _p_device: vk::PhysicalDevice) -> Result<()> {

    let properties = _instance.get_physical_device_properties(_p_device);
    if properties.device_type != vk::PhysicalDeviceType::DISCRETE_GPU 
    && properties.device_type != vk::PhysicalDeviceType::INTEGRATED_GPU {
        return Err(anyhow!(SuitabilityError("Only discrete and integrated GPUs are supported.")));
    }

    let features = _instance.get_physical_device_features(_p_device);
    if features.geometry_shader != vk::TRUE {
        return Err(anyhow!(SuitabilityError("Missing geometry shader support.")));
    }

    QueueFamilyIndices::get(_instance,_data,_p_device)?;

    return Ok(());

}

fn create_logical_decice(_entry: &Entry, _instance: &Instance, _data: &mut AppData) -> Result<Device> {

    let indices = QueueFamilyIndices::get(_instance, _data, _data.physical_device)?;

    let queue_priorities = &[1.0];
    let queue_info = vk::DeviceQueueCreateInfo::builder()
    .queue_family_index(indices.graphics)
    .queue_priorities(queue_priorities);

    let layers = if VALIDATION_ENABLED {
        vec![VALIDATION_LAYER.as_ptr()]
    } else {
        Vec::new()
    };

    let mut extensions = Vec::new();

    if cfg!(target_os = "macos") && _entry.version()? >= PORTABILITY_MACOS_VERSION {
        extensions.push(vk::KHR_PORTABILITY_SUBSET_EXTENSION.name.as_ptr());
    }

    let features = vk::PhysicalDeviceFeatures::builder();

    let queue_infos = &[queue_info];
    let info = vk::DeviceCreateInfo::builder()
    .queue_create_infos(queue_infos)
    .enabled_layer_names(&layers)
    .enabled_extension_names(&extensions)
    .enabled_features(&features);

    let device: Device;
    unsafe {
        device = _instance.create_device(_data.physical_device, &info, None)?;
    }

    unsafe {
        _data.graphics_queue = device.get_device_queue(indices.graphics, 0);
    }

    return Ok(device);

}

extern "system" fn debug_callback(severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    type_: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _: *mut c_void,
) -> vk::Bool32 {
    let data = unsafe { *data };
    let message = unsafe { CStr::from_ptr(data.message) }.to_string_lossy();
    
    if severity >= vk::DebugUtilsMessageSeverityFlagsEXT::ERROR {
        error!("({:?}) {}", type_, message);
    } else if severity >= vk::DebugUtilsMessageSeverityFlagsEXT::WARNING {
        warn!("({:?}) {}", type_, message);
    } else if severity >= vk::DebugUtilsMessageSeverityFlagsEXT::INFO {
        info!("({:?}) {}", type_, message);
    } else {
        trace!("({:?}) {}", type_, message);
    }

    return vk::FALSE;
}


fn create_instance(_window: &Window, _entry: &Entry, _data: &mut AppData) -> Result<Instance> {

    let application_info = vk::ApplicationInfo::builder()
    .application_name(b"Vulcan Tutorial\0")
    .application_version(vk::make_version(1, 0, 0))
    .engine_name(b"No Engine\0")
    .engine_version(vk::make_version(1, 0, 0))
    .api_version(vk::make_version(1, 0, 0));

    let available_layers: HashSet<vk::StringArray<256>>;
    unsafe {
         available_layers = _entry
        .enumerate_instance_layer_properties()?
        .iter()
        .map(|l| l.layer_name)
        .collect::<HashSet<_>>();
    }

    if VALIDATION_ENABLED && !available_layers.contains(&VALIDATION_LAYER) {
        return Err(anyhow!("Validation layer requested but not supported!"))
    }

    let layers = if VALIDATION_ENABLED {
        vec![VALIDATION_LAYER.as_ptr()]
    } else {
        Vec::new()
    };

    let mut extensions = vk_window::get_required_instance_extensions(_window)
    .iter()
    .map(|e| e.as_ptr())
    .collect::<Vec<_>>();

    if VALIDATION_ENABLED {
        extensions.push(vk::EXT_DEBUG_UTILS_EXTENSION.name.as_ptr());
    }

    let flags = if 
    cfg!(target_os = "macos") && _entry.version()? >= PORTABILITY_MACOS_VERSION {
        info!("Enabling extension for macOS Portability.");
        extensions.push(vk::KHR_GET_PHYSICAL_DEVICE_PROPERTIES2_EXTENSION.name.as_ptr());
        extensions.push(vk::KHR_PORTABILITY_ENUMERATION_EXTENSION.name.as_ptr());
        vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR
    } else {
        vk::InstanceCreateFlags::empty()
    };

    let mut info = vk::InstanceCreateInfo::builder()
    .application_info(&application_info)
    .enabled_layer_names(&layers)
    .enabled_extension_names(&extensions)
    .flags(flags);

    let mut debug_info = vk::DebugUtilsMessengerCreateInfoEXT::builder()
        .message_severity(vk::DebugUtilsMessageSeverityFlagsEXT::all())
        .message_type(
            vk::DebugUtilsMessageTypeFlagsEXT::GENERAL |
            vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION |
            vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
        )
        .user_callback(Some(debug_callback));

    if VALIDATION_ENABLED {
        info = info.push_next(&mut debug_info);
    }

    let instance: Instance;
    unsafe {
        instance = _entry.create_instance(&info, None)?;
    }

    Ok(instance)

}

fn main() -> Result<()> {
    std::env::set_var("RUST_LOG", "debug");
    pretty_env_logger::init();

    //Window generation
    let event_loop = EventLoop::new()?;
    let mut main_app = App::default();
    
    //App
    event_loop.run_app(&mut main_app)?;

    return Ok(());

}
