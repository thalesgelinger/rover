use std::ffi::{c_void, CStr};
use std::os::raw::c_char;
use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

use anyhow::{anyhow, Context, Result};
use crate::log_android;
use ash::extensions::{
    ext::DebugUtils,
    khr::{AndroidSurface, Surface, Swapchain},
};
use ash::vk;
use ash::vk::Handle;
use ndk::native_window::NativeWindow;

static INSTANCE_PROC_ADDR: OnceLock<vk::PFN_vkGetInstanceProcAddr> = OnceLock::new();
static DEVICE_PROC_ADDR: OnceLock<vk::PFN_vkGetDeviceProcAddr> = OnceLock::new();
static INSTANCE_PROC_MISS: AtomicUsize = AtomicUsize::new(0);
static DEVICE_PROC_MISS: AtomicUsize = AtomicUsize::new(0);

pub struct VulkanSession {
    entry: ash::Entry,
    instance: ash::Instance,
    physical_device: vk::PhysicalDevice,
    surface_loader: Surface,
    android_surface_loader: AndroidSurface,
    surface: vk::SurfaceKHR,
    device: ash::Device,
    queue: vk::Queue,
    queue_family_index: u32,
    swapchain_loader: Swapchain,
    swapchain: vk::SwapchainKHR,
    swapchain_images: Vec<vk::Image>,
    surface_format: vk::SurfaceFormatKHR,
    present_mode: vk::PresentModeKHR,
    image_format: vk::Format,
    extent: vk::Extent2D,
    image_available: Vec<vk::Semaphore>,
    render_finished: Vec<vk::Semaphore>,
    in_flight: Vec<vk::Fence>,
    command_pool: vk::CommandPool,
    command_buffers: Vec<vk::CommandBuffer>,
    current_frame: usize,
    pending_extent: Option<vk::Extent2D>,
    scale_factor: f32,
    window: Arc<NativeWindow>,
}

impl VulkanSession {
    pub fn new(window: NativeWindow, scale_factor: f32) -> Result<Self> {
        let window = Arc::new(window);
        let entry = unsafe { ash::Entry::linked() };
        let app_name = CStr::from_bytes_with_nul(b"rover\0").unwrap();
        let app_info = vk::ApplicationInfo::builder()
            .application_name(app_name)
            .application_version(0)
            .engine_name(app_name)
            .engine_version(0)
            .api_version(vk::API_VERSION_1_1);

        let extensions = vec![
            DebugUtils::name().as_ptr(),
            Surface::name().as_ptr(),
            AndroidSurface::name().as_ptr(),
        ];

        let instance_info = vk::InstanceCreateInfo::builder()
            .application_info(&app_info)
            .enabled_extension_names(&extensions);

        let instance = unsafe { entry.create_instance(&instance_info, None) }
            .context("create instance")?;

        INSTANCE_PROC_ADDR.get_or_init(|| entry.static_fn().get_instance_proc_addr);

        let surface_loader = Surface::new(&entry, &instance);
        let android_surface_loader = AndroidSurface::new(&entry, &instance);
        let surface = unsafe {
            let create_info = vk::AndroidSurfaceCreateInfoKHR::builder()
                .window(window.ptr().as_ptr() as *mut c_void);
            android_surface_loader
                .create_android_surface(&create_info, None)
                .context("create android surface")?
        };

        let (physical_device, queue_family_index, surface_caps) =
            pick_device(&instance, &surface_loader, surface)?;

        INSTANCE_PROC_ADDR.get_or_init(|| entry.static_fn().get_instance_proc_addr);

        let priorities = [1.0f32];
        let queue_info = vk::DeviceQueueCreateInfo::builder()
            .queue_family_index(queue_family_index)
            .queue_priorities(&priorities);

        let device_exts = [Swapchain::name().as_ptr()];
        let device_info = vk::DeviceCreateInfo::builder()
            .queue_create_infos(std::slice::from_ref(&queue_info))
            .enabled_extension_names(&device_exts);
        let device = unsafe { instance.create_device(physical_device, &device_info, None) }
            .context("create device")?;

        INSTANCE_PROC_ADDR.get_or_init(|| entry.static_fn().get_instance_proc_addr);
        DEVICE_PROC_ADDR.get_or_init(|| instance.fp_v1_0().get_device_proc_addr);

        let queue = unsafe { device.get_device_queue(queue_family_index, 0) };

        let swapchain_loader = Swapchain::new(&instance, &device);
        let surface_format =
            choose_surface_format(&instance, physical_device, surface, &surface_loader)?;
        let present_mode = choose_present_mode(&instance, physical_device, surface, &surface_loader);
        let extent = choose_extent(surface_caps, window.width() as u32, window.height() as u32);

        let swapchain = create_swapchain(
            &swapchain_loader,
            surface,
            surface_caps,
            surface_format,
            present_mode,
            extent,
            None,
        )?;

        let swapchain_images = unsafe { swapchain_loader.get_swapchain_images(swapchain) }
            .context("get swapchain images")?;

        let image_available = create_semaphores(&device, swapchain_images.len())?;
        let render_finished = create_semaphores(&device, swapchain_images.len())?;
        let in_flight = create_fences(&device, swapchain_images.len())?;

        let command_pool_info = vk::CommandPoolCreateInfo::builder()
            .queue_family_index(queue_family_index)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        let command_pool = unsafe { device.create_command_pool(&command_pool_info, None) }
            .context("command pool")?;

        let command_buffers = allocate_cmd_buffers(&device, command_pool, swapchain_images.len())?;

        Ok(Self {
            entry,
            instance,
            physical_device,
            surface_loader,
            android_surface_loader,
            surface,
            device,
            queue,
            queue_family_index,
            swapchain_loader,
            swapchain,
            swapchain_images,
            surface_format,
            present_mode,
            image_format: surface_format.format,
            extent,
            image_available,
            render_finished,
            in_flight,
            command_pool,
            command_buffers,
            current_frame: 0,
            pending_extent: None,
            scale_factor,
            window,
        })
    }

    pub fn request_resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.pending_extent = Some(vk::Extent2D { width, height });
        }
    }

    pub fn render_rgba(&mut self, runtime: *mut crate::RuntimeHandle) -> Result<bool> {
        let target_extent = vk::Extent2D {
            width: self.window.width() as u32,
            height: self.window.height() as u32,
        };

        if target_extent.width == 0 || target_extent.height == 0 {
            return Ok(false);
        }

        if let Some(extent) = self.pending_extent.take() {
            if extent != self.extent {
                self.recreate_swapchain(extent)?;
            }
        } else if target_extent != self.extent {
            self.recreate_swapchain(target_extent)?;
        }

        let idx = self.current_frame % self.image_available.len();
        let fence = self.in_flight[idx];
        unsafe {
            self.device
                .wait_for_fences(&[fence], true, u64::MAX)
                .context("wait fence")?;
            self.device.reset_fences(&[fence]).context("reset fence")?;
        }

        let (image_index, suboptimal) = match unsafe {
            self.swapchain_loader.acquire_next_image(
                self.swapchain,
                u64::MAX,
                self.image_available[idx],
                vk::Fence::null(),
            )
        } {
            Ok(res) => res,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                self.recreate_swapchain(target_extent)?;
                return Ok(false);
            }
            Err(err) => return Err(err).context("acquire next image"),
        };

        let image = self.swapchain_images[image_index as usize];
        self.transition_image(
            idx,
            image,
            vk::ImageLayout::PRESENT_SRC_KHR,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            std::slice::from_ref(&self.image_available[idx]),
            &[vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT],
            &[],
            vk::Fence::null(),
        )?;

        unsafe {
            self.device
                .queue_wait_idle(self.queue)
                .context("wait queue after to-color")?;
        }

        let format = self.image_format.as_raw() as u32;
        let layout = vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL.as_raw() as u32;
        let usage = (vk::ImageUsageFlags::COLOR_ATTACHMENT
            | vk::ImageUsageFlags::TRANSFER_DST
            | vk::ImageUsageFlags::TRANSFER_SRC
            | vk::ImageUsageFlags::SAMPLED)
            .as_raw() as u32;

        let rendered = unsafe {
            crate::rover_render_vulkan(
                runtime,
                self.instance.handle().as_raw() as *const c_void,
                self.physical_device.as_raw() as *const c_void,
                self.device.handle().as_raw() as *const c_void,
                self.queue.as_raw() as *const c_void,
                self.queue_family_index,
                image.as_raw() as *const c_void,
                format,
                layout,
                usage,
                self.extent.width as i32,
                self.extent.height as i32,
                1,
                self.scale_factor,
                get_instance_proc_addr,
                get_device_proc_addr,
            )
        };

        unsafe {
            self.device
                .queue_wait_idle(self.queue)
                .context("wait queue after render")?;
        }

        self.transition_image(

            idx,
            image,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::PRESENT_SRC_KHR,
            &[],
            &[],
            std::slice::from_ref(&self.render_finished[idx]),
            fence,
        )?;

        let present_info = vk::PresentInfoKHR::builder()
            .wait_semaphores(std::slice::from_ref(&self.render_finished[idx]))
            .swapchains(std::slice::from_ref(&self.swapchain))
            .image_indices(std::slice::from_ref(&image_index));

        let mut needs_recreate = suboptimal;
        match unsafe { self.swapchain_loader.queue_present(self.queue, &present_info) } {
            Ok(_) => {}
            Err(vk::Result::SUBOPTIMAL_KHR) | Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                needs_recreate = true;
            }
            Err(err) => return Err(err).context("queue present"),
        }

        self.current_frame = (self.current_frame + 1) % self.image_available.len();
        if needs_recreate {
            self.recreate_swapchain(target_extent)?;
        }
        Ok(rendered)
    }

    fn transition_image(
        &self,
        idx: usize,
        image: vk::Image,
        old_layout: vk::ImageLayout,
        new_layout: vk::ImageLayout,
        wait_semaphores: &[vk::Semaphore],
        wait_stages: &[vk::PipelineStageFlags],
        signal_semaphores: &[vk::Semaphore],
        fence: vk::Fence,
    ) -> Result<()> {
        let cmd = self.command_buffers[idx];
        record_transition(
            &self.device,
            cmd,
            image,
            self.queue_family_index,
            old_layout,
            new_layout,
        )?;

        let submit_info = vk::SubmitInfo::builder()
            .wait_semaphores(wait_semaphores)
            .wait_dst_stage_mask(wait_stages)
            .command_buffers(std::slice::from_ref(&cmd))
            .signal_semaphores(signal_semaphores);

        unsafe {
            self.device
                .queue_submit(self.queue, &[submit_info.build()], fence)
                .context("queue submit")?;
        }

        Ok(())
    }

    fn recreate_swapchain(&mut self, new_extent: vk::Extent2D) -> Result<()> {
        unsafe {
            self.device.device_wait_idle().ok();
        }

        self.destroy_swapchain_resources();

        let surface_caps = unsafe {
            self.surface_loader
                .get_physical_device_surface_capabilities(self.physical_device, self.surface)
        }
        .context("surface caps")?;

        let extent = choose_extent(surface_caps, new_extent.width, new_extent.height);
        let surface_format = choose_surface_format(
            &self.instance,
            self.physical_device,
            self.surface,
            &self.surface_loader,
        )?;
        let present_mode = choose_present_mode(
            &self.instance,
            self.physical_device,
            self.surface,
            &self.surface_loader,
        );

        let swapchain = create_swapchain(
            &self.swapchain_loader,
            self.surface,
            surface_caps,
            surface_format,
            present_mode,
            extent,
            None,
        )?;


        let swapchain_images = unsafe { self.swapchain_loader.get_swapchain_images(swapchain) }
            .context("get swapchain images")?;

        let image_available = create_semaphores(&self.device, swapchain_images.len())?;
        let render_finished = create_semaphores(&self.device, swapchain_images.len())?;
        let in_flight = create_fences(&self.device, swapchain_images.len())?;

        let command_pool_info = vk::CommandPoolCreateInfo::builder()
            .queue_family_index(self.queue_family_index)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        let command_pool = unsafe { self.device.create_command_pool(&command_pool_info, None) }
            .context("command pool")?;

        let command_buffers = allocate_cmd_buffers(&self.device, command_pool, swapchain_images.len())?;

        self.swapchain = swapchain;
        self.swapchain_images = swapchain_images;
        self.surface_format = surface_format;
        self.present_mode = present_mode;
        self.image_format = surface_format.format;
        self.extent = extent;
        self.image_available = image_available;
        self.render_finished = render_finished;
        self.in_flight = in_flight;
        self.command_pool = command_pool;
        self.command_buffers = command_buffers;
        self.current_frame = 0;

        Ok(())
    }

    fn destroy_swapchain_resources(&mut self) {
        unsafe {
            for &f in &self.in_flight {
                self.device.destroy_fence(f, None);
            }
            for &s in &self.image_available {
                self.device.destroy_semaphore(s, None);
            }
            for &s in &self.render_finished {
                self.device.destroy_semaphore(s, None);
            }
            if self.command_pool != vk::CommandPool::null() {
                self.device.destroy_command_pool(self.command_pool, None);
            }
            if self.swapchain != vk::SwapchainKHR::null() {
                self.swapchain_loader.destroy_swapchain(self.swapchain, None);
            }
        }

        self.in_flight.clear();
        self.image_available.clear();
        self.render_finished.clear();
        self.command_buffers.clear();
        self.swapchain_images.clear();
        self.swapchain = vk::SwapchainKHR::null();
        self.command_pool = vk::CommandPool::null();
    }

    pub fn width(&self) -> u32 {
        self.extent.width
    }

    pub fn height(&self) -> u32 {
        self.extent.height
    }
}

impl Drop for VulkanSession {
    fn drop(&mut self) {
        unsafe {
            self.device.device_wait_idle().ok();
        }
        self.destroy_swapchain_resources();
        unsafe {
            self.device.destroy_device(None);
            self.surface_loader.destroy_surface(self.surface, None);
            self.instance.destroy_instance(None);
        }
    }
}

pub(crate) unsafe extern "system" fn get_instance_proc_addr(
    instance: *const c_void,
    name: *const u8,
) -> *const c_void {
    let func = match INSTANCE_PROC_ADDR.get() {
        Some(f) => *f,
        None => return ptr::null(),
    };
    let instance = if instance.is_null() {
        vk::Instance::null()
    } else {
        vk::Instance::from_raw(instance as u64)
    };
    let ptr = match func(instance, name as *const u8) {
        Some(p) => p as *const c_void,
        None => ptr::null(),
    };
    if ptr.is_null() && !name.is_null() {
        if INSTANCE_PROC_MISS.fetch_add(1, Ordering::Relaxed) < 5 {
            if let Ok(s) = CStr::from_ptr(name as *const c_char).to_str() {
                log_android("Rover", &format!("missing instance proc {s}"));
            }
        }
    }
    ptr
}

pub(crate) unsafe extern "system" fn get_device_proc_addr(
    device: *const c_void,
    name: *const u8,
) -> *const c_void {
    let func = match DEVICE_PROC_ADDR.get() {
        Some(f) => *f,
        None => return ptr::null(),
    };
    let device = if device.is_null() {
        vk::Device::null()
    } else {
        vk::Device::from_raw(device as u64)
    };
    let ptr = match func(device, name as *const u8) {
        Some(p) => p as *const c_void,
        None => ptr::null(),
    };
    if ptr.is_null() && !name.is_null() {
        if DEVICE_PROC_MISS.fetch_add(1, Ordering::Relaxed) < 5 {
            if let Ok(s) = CStr::from_ptr(name as *const c_char).to_str() {
                log_android("Rover", &format!("missing device proc {s}"));
            }
        }
    }
    ptr
}

fn pick_device(
    instance: &ash::Instance,
    surface_loader: &Surface,
    surface: vk::SurfaceKHR,
) -> Result<(vk::PhysicalDevice, u32, vk::SurfaceCapabilitiesKHR)> {
    let devices = unsafe { instance.enumerate_physical_devices() }.context("enumerate devices")?;
    for device in devices {
        let queue_props = unsafe { instance.get_physical_device_queue_family_properties(device) };
        for (index, info) in queue_props.iter().enumerate() {
            if !info.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
                continue;
            }
            let supports_present = unsafe {
                surface_loader.get_physical_device_surface_support(device, index as u32, surface)
            }?;
            if supports_present {
                let caps = unsafe {
                    surface_loader.get_physical_device_surface_capabilities(device, surface)
                }?;
                return Ok((device, index as u32, caps));
            }
        }
    }
    Err(anyhow!("no suitable GPU"))
}

fn choose_surface_format(
    _instance: &ash::Instance,
    device: vk::PhysicalDevice,
    surface: vk::SurfaceKHR,
    surface_loader: &Surface,
) -> Result<vk::SurfaceFormatKHR> {
    let formats = unsafe {
        surface_loader.get_physical_device_surface_formats(device, surface)
    }?;
    for f in &formats {
        if f.format == vk::Format::B8G8R8A8_UNORM && f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR {
            return Ok(*f);
        }
    }
    formats.get(0).copied().ok_or_else(|| anyhow!("no surface formats"))
}

fn choose_present_mode(
    _instance: &ash::Instance,
    device: vk::PhysicalDevice,
    surface: vk::SurfaceKHR,
    surface_loader: &Surface,
) -> vk::PresentModeKHR {
    if let Ok(modes) = unsafe {
        surface_loader.get_physical_device_surface_present_modes(device, surface)
    } {
        if modes.contains(&vk::PresentModeKHR::MAILBOX) {
            return vk::PresentModeKHR::MAILBOX;
        }
    }
    vk::PresentModeKHR::FIFO
}

fn choose_extent(
    caps: vk::SurfaceCapabilitiesKHR,
    width: u32,
    height: u32,
) -> vk::Extent2D {
    if caps.current_extent.width != u32::MAX {
        caps.current_extent
    } else {
        vk::Extent2D {
            width: width.clamp(caps.min_image_extent.width, caps.max_image_extent.width),
            height: height.clamp(caps.min_image_extent.height, caps.max_image_extent.height),
        }
    }
}

fn create_swapchain(
    loader: &Swapchain,
    surface: vk::SurfaceKHR,
    caps: vk::SurfaceCapabilitiesKHR,
    format: vk::SurfaceFormatKHR,
    present_mode: vk::PresentModeKHR,
    extent: vk::Extent2D,
    old_swapchain: Option<vk::SwapchainKHR>,
) -> Result<vk::SwapchainKHR> {
    let mut image_count = caps.min_image_count + 1;
    if caps.max_image_count > 0 && image_count > caps.max_image_count {
        image_count = caps.max_image_count;
    }
    let create_info = vk::SwapchainCreateInfoKHR::builder()
        .surface(surface)
        .min_image_count(image_count)
        .image_format(format.format)
        .image_color_space(format.color_space)
        .image_extent(extent)
        .image_array_layers(1)
        .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_DST)
        .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
        .pre_transform(caps.current_transform)
        .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
        .present_mode(present_mode)
        .clipped(true)
        .old_swapchain(old_swapchain.unwrap_or(vk::SwapchainKHR::null()));
    unsafe { loader.create_swapchain(&create_info, None) }.context("create swapchain")
}

fn create_semaphores(device: &ash::Device, count: usize) -> Result<Vec<vk::Semaphore>> {
    let info = vk::SemaphoreCreateInfo::default();
    let mut sems = Vec::with_capacity(count);
    for _ in 0..count {
        sems.push(unsafe { device.create_semaphore(&info, None) }.context("semaphore")?);
    }
    Ok(sems)
}

fn create_fences(device: &ash::Device, count: usize) -> Result<Vec<vk::Fence>> {
    let info = vk::FenceCreateInfo::builder().flags(vk::FenceCreateFlags::SIGNALED);
    let mut fences = Vec::with_capacity(count);
    for _ in 0..count {
        fences.push(unsafe { device.create_fence(&info, None) }.context("fence")?);
    }
    Ok(fences)
}

fn allocate_cmd_buffers(
    device: &ash::Device,
    pool: vk::CommandPool,
    count: usize,
) -> Result<Vec<vk::CommandBuffer>> {
    let alloc_info = vk::CommandBufferAllocateInfo::builder()
        .command_pool(pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(count as u32);
    let bufs = unsafe { device.allocate_command_buffers(&alloc_info) }.context("alloc cmds")?;
    Ok(bufs)
}

fn record_transition(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    image: vk::Image,
    queue_family_index: u32,
    old_layout: vk::ImageLayout,
    new_layout: vk::ImageLayout,
) -> Result<()> {
    let (src_access, dst_access, src_stage, dst_stage) = match (old_layout, new_layout) {
        (vk::ImageLayout::PRESENT_SRC_KHR, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL) => (
            vk::AccessFlags::MEMORY_READ,
            vk::AccessFlags::COLOR_ATTACHMENT_WRITE | vk::AccessFlags::COLOR_ATTACHMENT_READ,
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
        ),
        (vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL, vk::ImageLayout::PRESENT_SRC_KHR) => (
            vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            vk::AccessFlags::empty(),
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            vk::PipelineStageFlags::BOTTOM_OF_PIPE,
        ),
        _ => (
            vk::AccessFlags::empty(),
            vk::AccessFlags::empty(),
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::PipelineStageFlags::BOTTOM_OF_PIPE,
        ),
    };

    unsafe {
        device
            .reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())
            .context("reset cmd")?;
        let begin = vk::CommandBufferBeginInfo::builder();
        device.begin_command_buffer(cmd, &begin).context("begin cmd")?;

        let barrier = vk::ImageMemoryBarrier::builder()
            .old_layout(old_layout)
            .new_layout(new_layout)
            .src_queue_family_index(queue_family_index)
            .dst_queue_family_index(queue_family_index)
            .image(image)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            })
            .src_access_mask(src_access)
            .dst_access_mask(dst_access);

        device.cmd_pipeline_barrier(
            cmd,
            src_stage,
            dst_stage,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[*barrier],
        );

        device.end_command_buffer(cmd).context("end cmd")?;
    }
    Ok(())
}
