//! Pipeline

use crate::memory::{DeviceMemory, MemoryBinding};
use crate::{Context, LogicalDevice, NonDispatchable};
use headers::vk_decls::*;
use log::*;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;

#[derive(Debug)]
pub struct PipelineLayout {
    handle: VkNonDispatchableHandle,
    logical_device: Arc<Mutex<LogicalDevice>>,
}

impl PipelineLayout {
    pub fn create(
        logical_device: Arc<Mutex<LogicalDevice>>,
        flags: VkDescriptorSetLayoutCreateFlags,
        set_layouts: Option<&[VkDescriptorSetLayout]>,
        push_constant_ranges: Option<&[VkPushConstantRange]>,
    ) -> VkNonDispatchableHandle {
        info!("new PipelineLayout");
        let handle = VK_NULL_HANDLE;

        let _ = flags;
        let _ = set_layouts;
        let _ = push_constant_ranges;

        let object = Self {
            handle,
            logical_device,
        };
        object.register_object()
    }
}

impl NonDispatchable for PipelineLayout {
    fn get_hash(context: &Context) -> &HashMap<VkNonDispatchableHandle, Arc<Mutex<Self>>> {
        &context.pipeline_layouts
    }

    fn get_hash_mut(
        context: &mut Context,
    ) -> &mut HashMap<VkNonDispatchableHandle, Arc<Mutex<Self>>> {
        &mut context.pipeline_layouts
    }

    fn set_handle(&mut self, handle: VkNonDispatchableHandle) {
        self.handle = handle;
    }

    fn get_handle(&self) -> VkNonDispatchableHandle {
        self.handle
    }
}

#[derive(Debug)]
pub struct RenderPass {
    handle: VkNonDispatchableHandle,
    logical_device: Arc<Mutex<LogicalDevice>>,
}

impl RenderPass {
    pub fn create(
        logical_device: Arc<Mutex<LogicalDevice>>,
        flags: VkDescriptorSetLayoutCreateFlags,
        attachments: Option<&[VkAttachmentDescription]>,
        dependencies: Option<&[VkSubpassDependency]>,
        subpasses: Option<&[VkSubpassDescription]>,
    ) -> VkNonDispatchableHandle {
        info!("new RenderPass");
        let handle = VK_NULL_HANDLE;

        let _ = flags;
        let _ = attachments;
        let _ = dependencies;
        let _ = subpasses;

        let object = Self {
            handle,
            logical_device,
        };
        object.register_object()
    }
}

impl NonDispatchable for RenderPass {
    fn get_hash(context: &Context) -> &HashMap<VkNonDispatchableHandle, Arc<Mutex<Self>>> {
        &context.render_passes
    }

    fn get_hash_mut(
        context: &mut Context,
    ) -> &mut HashMap<VkNonDispatchableHandle, Arc<Mutex<Self>>> {
        &mut context.render_passes
    }

    fn set_handle(&mut self, handle: VkNonDispatchableHandle) {
        self.handle = handle;
    }

    fn get_handle(&self) -> VkNonDispatchableHandle {
        self.handle
    }
}

#[derive(Debug)]
pub struct ShaderModule {
    handle: VkNonDispatchableHandle,
    logical_device: Arc<Mutex<LogicalDevice>>,
    code: Vec<u32>,
}

impl ShaderModule {
    pub fn create(
        logical_device: Arc<Mutex<LogicalDevice>>,
        flags: VkDescriptorSetLayoutCreateFlags,
        code: &[u32],
    ) -> VkNonDispatchableHandle {
        info!("new ShaderModule");
        let handle = VK_NULL_HANDLE;

        let _ = flags;
        let code = code.to_vec();

        let object = Self {
            handle,
            logical_device,
            code,
        };
        object.register_object()
    }
}

impl NonDispatchable for ShaderModule {
    fn get_hash(context: &Context) -> &HashMap<VkNonDispatchableHandle, Arc<Mutex<Self>>> {
        &context.shader_modules
    }

    fn get_hash_mut(
        context: &mut Context,
    ) -> &mut HashMap<VkNonDispatchableHandle, Arc<Mutex<Self>>> {
        &mut context.shader_modules
    }

    fn set_handle(&mut self, handle: VkNonDispatchableHandle) {
        self.handle = handle;
    }

    fn get_handle(&self) -> VkNonDispatchableHandle {
        self.handle
    }
}

#[derive(Debug)]
pub struct PipelineCache {
    handle: VkNonDispatchableHandle,
    logical_device: Arc<Mutex<LogicalDevice>>,
    initial_data: Vec<u8>,
}

impl PipelineCache {
    pub fn create(
        logical_device: Arc<Mutex<LogicalDevice>>,
        flags: VkDescriptorSetLayoutCreateFlags,
        initial_data: &[u8],
    ) -> VkNonDispatchableHandle {
        info!("new PipelineCache");
        let handle = VK_NULL_HANDLE;

        let _ = flags;
        let initial_data = initial_data.to_vec();

        let object = Self {
            handle,
            logical_device,
            initial_data,
        };
        object.register_object()
    }
}

impl NonDispatchable for PipelineCache {
    fn get_hash(context: &Context) -> &HashMap<VkNonDispatchableHandle, Arc<Mutex<Self>>> {
        &context.pipeline_caches
    }

    fn get_hash_mut(
        context: &mut Context,
    ) -> &mut HashMap<VkNonDispatchableHandle, Arc<Mutex<Self>>> {
        &mut context.pipeline_caches
    }

    fn set_handle(&mut self, handle: VkNonDispatchableHandle) {
        self.handle = handle;
    }

    fn get_handle(&self) -> VkNonDispatchableHandle {
        self.handle
    }
}
