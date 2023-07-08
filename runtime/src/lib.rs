pub mod command_buffer;
pub mod image;
pub mod surface;
pub mod swapchain;

use headers::c_char_array;
use headers::vk_decls::*;
use lazy_static::lazy_static;
use log::*;
use parking_lot::{
    MappedRwLockReadGuard, MappedRwLockWriteGuard, Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard,
};
use std::collections::HashMap;
use std::ffi::c_char;
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/* Context */

#[derive(Debug, Default)]
pub struct Context {
    instances: Vec<Arc<Instance>>,
    physical_devices: Vec<Arc<PhysicalDevice>>,
    logical_devices: Vec<Arc<LogicalDevice>>,
    queues: Vec<Arc<Queue>>,
    fences: HashMap<VkNonDispatchableHandle, Arc<Mutex<Fence>>>,
    semaphores: HashMap<VkNonDispatchableHandle, Arc<Mutex<Semaphore>>>,
    surfaces: HashMap<VkNonDispatchableHandle, Arc<Mutex<surface::Surface>>>,
    swapchains: HashMap<VkNonDispatchableHandle, Arc<Mutex<swapchain::Swapchain>>>,
    images: HashMap<VkNonDispatchableHandle, Arc<Mutex<image::Image>>>,
    image_views: HashMap<VkNonDispatchableHandle, Arc<Mutex<image::ImageView>>>,
    command_pools: HashMap<VkNonDispatchableHandle, Arc<Mutex<command_buffer::CommandPool>>>,
}

impl Context {
    pub fn new() -> Self {
        Default::default()
    }
}

lazy_static! {
    static ref CONTEXT: RwLock<Context> = RwLock::new(Context::new());
}

pub trait Dispatchable<T = Self> {
    fn get_vec<'a>(&'a self, context: &'a mut RwLockWriteGuard<Context>) -> &mut Vec<Arc<Self>>;

    fn register_object(self: Arc<Self>) -> Arc<Self> {
        let mut context: RwLockWriteGuard<'_, _> = CONTEXT.write();
        let context = &mut context;
        self.get_vec(context).push(self.clone());
        self
    }

    fn unregister_object(self: &Arc<Self>) {
        let mut context = CONTEXT.write();
        let index = self
            .get_vec(&mut context)
            .iter()
            .position(|x| Arc::ptr_eq(x, self))
            .unwrap();
        self.get_vec(&mut context).remove(index);
    }

    unsafe fn set_ffi_handle(handle: NonNull<VkDispatchableHandle>, value: Arc<T>) {
        trace!(
            "{}::set_ffi_handle arc: {} {}",
            std::any::type_name::<T>(),
            Arc::strong_count(&value),
            Arc::weak_count(&value)
        );
        let value = Arc::into_raw(value);
        Arc::decrement_strong_count(value);
        *handle.as_ptr() = std::mem::transmute(value);
    }

    unsafe fn from_handle(handle: VkDispatchableHandle) -> Option<Arc<T>> {
        handle.map_or_else(
            || None,
            |handle| {
                let ptr = std::mem::transmute::<_, *const T>(handle);
                Arc::increment_strong_count(ptr);
                let arc = Arc::from_raw(ptr);
                trace!(
                    "{}::from_handle arc: {} {}",
                    std::any::type_name::<Self>(),
                    Arc::strong_count(&arc),
                    Arc::weak_count(&arc)
                );
                Some(arc)
            },
        )
    }

    fn drop_handle(self: Arc<Self>) {
        self.unregister_object();
        assert_eq!(Arc::strong_count(&self), 1);
        drop(self);
    }
}

static ID_COUNTER: AtomicU64 = AtomicU64::new(1);

pub trait NonDispatchable<T = Self>
where
    Self: Sized,
{
    fn get_hash<'a>(context: &'a Context)
        -> &'a HashMap<VkNonDispatchableHandle, Arc<Mutex<Self>>>;

    fn get_hash_mut<'a>(
        context: &'a mut Context,
    ) -> &'a mut HashMap<VkNonDispatchableHandle, Arc<Mutex<Self>>>;

    fn set_handle(&mut self, handle: VkNonDispatchableHandle);
    fn get_handle(&self) -> VkNonDispatchableHandle;

    fn register_object(self) -> VkNonDispatchableHandle {
        let mut context: RwLockWriteGuard<'_, _> = CONTEXT.write();
        let context = &mut context;
        let handle =
            VkNonDispatchableHandle(NonZeroU64::new(ID_COUNTER.fetch_add(1, Ordering::Relaxed)));
        let object = Arc::new(Mutex::new(self));
        Self::get_hash_mut(context).insert(handle, object.clone());
        object.lock().set_handle(handle);
        handle
    }

    fn from_handle(handle: VkNonDispatchableHandle) -> Arc<Mutex<Self>> {
        let context = CONTEXT.read();
        Self::get_hash(&context).get(&handle).unwrap().clone()
    }

    fn drop_handle(handle: VkNonDispatchableHandle) {
        let mut context = CONTEXT.write();
        Self::get_hash_mut(&mut context).remove(&handle);
    }
}

/* Instance */

/// Contains system-level information and functionality
#[derive(Debug)]
pub struct Instance {
    driver_name: &'static str,
}
impl Instance {
    pub fn new() -> Arc<Self> {
        let instance = Self {
            driver_name: "vulkan_software_rasterizer",
        };
        let instance = Arc::new(instance);
        instance.register_object()
    }

    pub fn extension_count() -> usize {
        Self::extension_properties().len()
    }

    pub fn extension_properties() -> [VkExtensionProperties; 2] {
        c_char_array!(
            VK_KHR_SURFACE_EXTENSION_NAME,
            VK_MAX_EXTENSION_NAME_SIZE,
            "VK_KHR_surface"
        );
        c_char_array!(
            VK_KHR_XCB_SURFACE_EXTENSION_NAME,
            VK_MAX_EXTENSION_NAME_SIZE,
            "VK_KHR_xcb_surface"
        );
        [
            VkExtensionProperties {
                extensionName: *VK_KHR_SURFACE_EXTENSION_NAME,
                specVersion: 25,
            },
            VkExtensionProperties {
                extensionName: *VK_KHR_XCB_SURFACE_EXTENSION_NAME,
                specVersion: 6,
            },
        ]
    }
}

impl Dispatchable for Instance {
    fn get_vec<'a>(&'a self, context: &'a mut RwLockWriteGuard<Context>) -> &mut Vec<Arc<Self>> {
        context.instances.as_mut()
    }
}

/* PhysicalDevice */

/// Performs rendering operations.
#[derive(Debug)]
pub struct PhysicalDevice {
    physical_device_name: &'static str,
}

impl PhysicalDevice {
    pub fn get() -> Arc<Self> {
        info!("new PhysicalDevice");
        let mut context = CONTEXT.write();
        if context.physical_devices.len() < Self::physical_device_count() {
            let physical_device = Self {
                physical_device_name: "vulkan_software_rasterizer physical device",
            };
            let physical_device = Arc::new(physical_device);

            context.physical_devices.push(physical_device.clone());
        }
        let arc = context.physical_devices.last().unwrap().clone();
        arc
    }

    pub const fn physical_device_count() -> usize {
        1
    }

    pub fn extension_count() -> usize {
        Self::extension_properties().len()
    }

    pub fn extension_properties() -> [VkExtensionProperties; 1] {
        c_char_array!(
            VK_KHR_SWAPCHAIN_EXTENSION_NAME,
            VK_MAX_EXTENSION_NAME_SIZE,
            "VK_KHR_swapchain"
        );
        [VkExtensionProperties {
            extensionName: *VK_KHR_SWAPCHAIN_EXTENSION_NAME,
            specVersion: 70,
        }]
    }

    pub fn properties(&self) -> VkPhysicalDeviceProperties {
        c_char_array!(
            DEVICE_NAME,
            VK_MAX_PHYSICAL_DEVICE_NAME_SIZE,
            "vulkan_software_rasterizer physical device"
        );

        VkPhysicalDeviceProperties {
            apiVersion: 0,
            driverVersion: 0,
            vendorID: 0,
            deviceID: 0,
            deviceType: VkPhysicalDeviceType::VK_PHYSICAL_DEVICE_TYPE_OTHER,
            deviceName: *DEVICE_NAME,
            pipelineCacheUUID: [
                0x0, 0x1, 0x2, 0x3, 0x4, 0x5, 0x6, 0x7, 0x8, 0x9, 0x10, 0x11, 0x12, 0x13, 0x14,
                0x15,
            ],
            limits: VkPhysicalDeviceLimits {
                maxImageDimension1D: 0,
                maxImageDimension2D: 0,
                maxImageDimension3D: 0,
                maxImageDimensionCube: 0,
                maxImageArrayLayers: 0,
                maxTexelBufferElements: 0,
                maxUniformBufferRange: 0,
                maxStorageBufferRange: 0,
                maxPushConstantsSize: 0,
                maxMemoryAllocationCount: 0,
                maxSamplerAllocationCount: 0,
                bufferImageGranularity: 0,
                sparseAddressSpaceSize: 0,
                maxBoundDescriptorSets: 0,
                maxPerStageDescriptorSamplers: 0,
                maxPerStageDescriptorUniformBuffers: 0,
                maxPerStageDescriptorStorageBuffers: 0,
                maxPerStageDescriptorSampledImages: 0,
                maxPerStageDescriptorStorageImages: 0,
                maxPerStageDescriptorInputAttachments: 0,
                maxPerStageResources: 0,
                maxDescriptorSetSamplers: 0,
                maxDescriptorSetUniformBuffers: 0,
                maxDescriptorSetUniformBuffersDynamic: 0,
                maxDescriptorSetStorageBuffers: 0,
                maxDescriptorSetStorageBuffersDynamic: 0,
                maxDescriptorSetSampledImages: 0,
                maxDescriptorSetStorageImages: 0,
                maxDescriptorSetInputAttachments: 0,
                maxVertexInputAttributes: 0,
                maxVertexInputBindings: 0,
                maxVertexInputAttributeOffset: 0,
                maxVertexInputBindingStride: 0,
                maxVertexOutputComponents: 0,
                maxTessellationGenerationLevel: 0,
                maxTessellationPatchSize: 0,
                maxTessellationControlPerVertexInputComponents: 0,
                maxTessellationControlPerVertexOutputComponents: 0,
                maxTessellationControlPerPatchOutputComponents: 0,
                maxTessellationControlTotalOutputComponents: 0,
                maxTessellationEvaluationInputComponents: 0,
                maxTessellationEvaluationOutputComponents: 0,
                maxGeometryShaderInvocations: 0,
                maxGeometryInputComponents: 0,
                maxGeometryOutputComponents: 0,
                maxGeometryOutputVertices: 0,
                maxGeometryTotalOutputComponents: 0,
                maxFragmentInputComponents: 0,
                maxFragmentOutputAttachments: 0,
                maxFragmentDualSrcAttachments: 0,
                maxFragmentCombinedOutputResources: 0,
                maxComputeSharedMemorySize: 0,
                maxComputeWorkGroupCount: [0, 0, 0],
                maxComputeWorkGroupInvocations: 0,
                maxComputeWorkGroupSize: [0, 0, 0],
                subPixelPrecisionBits: 0,
                subTexelPrecisionBits: 0,
                mipmapPrecisionBits: 0,
                maxDrawIndexedIndexValue: 0,
                maxDrawIndirectCount: 0,
                maxSamplerLodBias: 0.0,
                maxSamplerAnisotropy: 0.0,
                maxViewports: 0,
                maxViewportDimensions: [0, 0],
                viewportBoundsRange: [0.0, 0.0],
                viewportSubPixelBits: 0,
                minMemoryMapAlignment: 0,
                minTexelBufferOffsetAlignment: 0,
                minUniformBufferOffsetAlignment: 0,
                minStorageBufferOffsetAlignment: 0,
                minTexelOffset: 0,
                maxTexelOffset: 0,
                minTexelGatherOffset: 0,
                maxTexelGatherOffset: 0,
                minInterpolationOffset: 0.0,
                maxInterpolationOffset: 0.0,
                subPixelInterpolationOffsetBits: 0,
                maxFramebufferWidth: 0,
                maxFramebufferHeight: 0,
                maxFramebufferLayers: 0,
                framebufferColorSampleCounts: 0,
                framebufferDepthSampleCounts: 0,
                framebufferStencilSampleCounts: 0,
                framebufferNoAttachmentsSampleCounts: 0,
                maxColorAttachments: 0,
                sampledImageColorSampleCounts: 0,
                sampledImageIntegerSampleCounts: 0,
                sampledImageDepthSampleCounts: 0,
                sampledImageStencilSampleCounts: 0,
                storageImageSampleCounts: 0,
                maxSampleMaskWords: 0,
                timestampComputeAndGraphics: 0,
                timestampPeriod: 0.0,
                maxClipDistances: 0,
                maxCullDistances: 0,
                maxCombinedClipAndCullDistances: 0,
                discreteQueuePriorities: 0,
                pointSizeRange: [0.0, 0.0],
                lineWidthRange: [0.0, 0.0],
                pointSizeGranularity: 0.0,
                lineWidthGranularity: 0.0,
                strictLines: 0,
                standardSampleLocations: 0,
                optimalBufferCopyOffsetAlignment: 0,
                optimalBufferCopyRowPitchAlignment: 0,
                nonCoherentAtomSize: 0,
            },
            sparseProperties: VkPhysicalDeviceSparseProperties {
                residencyStandard2DBlockShape: 0,
                residencyStandard2DMultisampleBlockShape: 0,
                residencyStandard3DBlockShape: 0,
                residencyAlignedMipSize: 0,
                residencyNonResidentStrict: 0,
            },
        }
    }

    pub fn memory_properties(&self) -> VkPhysicalDeviceMemoryProperties {
        lazy_static! {
            static ref MEMORY_TYPES: [VkMemoryType; VK_MAX_MEMORY_TYPES as usize] = {
                let m: [VkMemoryType; VK_MAX_MEMORY_TYPES as usize] =
                    [VkMemoryType {propertyFlags: 0, heapIndex: 0}; VK_MAX_MEMORY_TYPES as usize];
                // TODO: Fill in memory types.
                m
            };
            static ref MEMORY_HEAPS: [VkMemoryHeap; VK_MAX_MEMORY_HEAPS as usize] = {
                let m: [VkMemoryHeap; VK_MAX_MEMORY_HEAPS as usize] =
                    [VkMemoryHeap {size: 0, flags: 0}; VK_MAX_MEMORY_HEAPS as usize];
                // TODO: Fill in memory heaps.
                m
            };
        }
        VkPhysicalDeviceMemoryProperties {
            memoryTypeCount: 0,
            memoryTypes: *MEMORY_TYPES,
            memoryHeapCount: 0,
            memoryHeaps: *MEMORY_HEAPS,
        }
    }

    pub const fn features(&self) -> VkPhysicalDeviceFeatures {
        VkPhysicalDeviceFeatures {
            robustBufferAccess: VK_FALSE,
            fullDrawIndexUint32: VK_FALSE,
            imageCubeArray: VK_FALSE,
            independentBlend: VK_FALSE,
            geometryShader: VK_FALSE,
            tessellationShader: VK_FALSE,
            sampleRateShading: VK_FALSE,
            dualSrcBlend: VK_FALSE,
            logicOp: VK_FALSE,
            multiDrawIndirect: VK_FALSE,
            drawIndirectFirstInstance: VK_FALSE,
            depthClamp: VK_FALSE,
            depthBiasClamp: VK_FALSE,
            fillModeNonSolid: VK_FALSE,
            depthBounds: VK_FALSE,
            wideLines: VK_FALSE,
            largePoints: VK_FALSE,
            alphaToOne: VK_FALSE,
            multiViewport: VK_FALSE,
            samplerAnisotropy: VK_FALSE,
            textureCompressionETC2: VK_FALSE,
            textureCompressionASTC_LDR: VK_FALSE,
            textureCompressionBC: VK_FALSE,
            occlusionQueryPrecise: VK_FALSE,
            pipelineStatisticsQuery: VK_FALSE,
            vertexPipelineStoresAndAtomics: VK_FALSE,
            fragmentStoresAndAtomics: VK_FALSE,
            shaderTessellationAndGeometryPointSize: VK_FALSE,
            shaderImageGatherExtended: VK_FALSE,
            shaderStorageImageExtendedFormats: VK_FALSE,
            shaderStorageImageMultisample: VK_FALSE,
            shaderStorageImageReadWithoutFormat: VK_FALSE,
            shaderStorageImageWriteWithoutFormat: VK_FALSE,
            shaderUniformBufferArrayDynamicIndexing: VK_FALSE,
            shaderSampledImageArrayDynamicIndexing: VK_FALSE,
            shaderStorageBufferArrayDynamicIndexing: VK_FALSE,
            shaderStorageImageArrayDynamicIndexing: VK_FALSE,
            shaderClipDistance: VK_FALSE,
            shaderCullDistance: VK_FALSE,
            shaderFloat64: VK_FALSE,
            shaderInt64: VK_FALSE,
            shaderInt16: VK_FALSE,
            shaderResourceResidency: VK_FALSE,
            shaderResourceMinLod: VK_FALSE,
            sparseBinding: VK_FALSE,
            sparseResidencyBuffer: VK_FALSE,
            sparseResidencyImage2D: VK_FALSE,
            sparseResidencyImage3D: VK_FALSE,
            sparseResidency2Samples: VK_FALSE,
            sparseResidency4Samples: VK_FALSE,
            sparseResidency8Samples: VK_FALSE,
            sparseResidency16Samples: VK_FALSE,
            sparseResidencyAliased: VK_FALSE,
            variableMultisampleRate: VK_FALSE,
            inheritedQueries: VK_FALSE,
        }
    }

    pub const fn format_properties(&self, format: VkFormat) -> VkFormatProperties {
        let unsupported = VkFormatProperties {
            linearTilingFeatures: 0,
            optimalTilingFeatures: 0,
            bufferFeatures: 0,
        };
        match format {
            VkFormat::VK_FORMAT_UNDEFINED => unsupported,
            VkFormat::VK_FORMAT_R4G4_UNORM_PACK8 => unsupported,
            VkFormat::VK_FORMAT_R4G4B4A4_UNORM_PACK16 => unsupported,
            VkFormat::VK_FORMAT_B4G4R4A4_UNORM_PACK16 => unsupported,
            VkFormat::VK_FORMAT_R5G6B5_UNORM_PACK16 => unsupported,
            VkFormat::VK_FORMAT_B5G6R5_UNORM_PACK16 => unsupported,
            VkFormat::VK_FORMAT_R5G5B5A1_UNORM_PACK16 => unsupported,
            VkFormat::VK_FORMAT_B5G5R5A1_UNORM_PACK16 => unsupported,
            VkFormat::VK_FORMAT_A1R5G5B5_UNORM_PACK16 => unsupported,
            VkFormat::VK_FORMAT_R8_UNORM => unsupported,
            VkFormat::VK_FORMAT_R8_SNORM => unsupported,
            VkFormat::VK_FORMAT_R8_USCALED => unsupported,
            VkFormat::VK_FORMAT_R8_SSCALED => unsupported,
            VkFormat::VK_FORMAT_R8_UINT => unsupported,
            VkFormat::VK_FORMAT_R8_SINT => unsupported,
            VkFormat::VK_FORMAT_R8_SRGB => unsupported,
            VkFormat::VK_FORMAT_R8G8_UNORM => unsupported,
            VkFormat::VK_FORMAT_R8G8_SNORM => unsupported,
            VkFormat::VK_FORMAT_R8G8_USCALED => unsupported,
            VkFormat::VK_FORMAT_R8G8_SSCALED => unsupported,
            VkFormat::VK_FORMAT_R8G8_UINT => unsupported,
            VkFormat::VK_FORMAT_R8G8_SINT => unsupported,
            VkFormat::VK_FORMAT_R8G8_SRGB => unsupported,
            VkFormat::VK_FORMAT_R8G8B8_UNORM => unsupported,
            VkFormat::VK_FORMAT_R8G8B8_SNORM => unsupported,
            VkFormat::VK_FORMAT_R8G8B8_USCALED => unsupported,
            VkFormat::VK_FORMAT_R8G8B8_SSCALED => unsupported,
            VkFormat::VK_FORMAT_R8G8B8_UINT => unsupported,
            VkFormat::VK_FORMAT_R8G8B8_SINT => unsupported,
            VkFormat::VK_FORMAT_R8G8B8_SRGB => unsupported,
            VkFormat::VK_FORMAT_B8G8R8_UNORM => unsupported,
            VkFormat::VK_FORMAT_B8G8R8_SNORM => unsupported,
            VkFormat::VK_FORMAT_B8G8R8_USCALED => unsupported,
            VkFormat::VK_FORMAT_B8G8R8_SSCALED => unsupported,
            VkFormat::VK_FORMAT_B8G8R8_UINT => unsupported,
            VkFormat::VK_FORMAT_B8G8R8_SINT => unsupported,
            VkFormat::VK_FORMAT_B8G8R8_SRGB => unsupported,
            VkFormat::VK_FORMAT_R8G8B8A8_UNORM => unsupported,
            VkFormat::VK_FORMAT_R8G8B8A8_SNORM => unsupported,
            VkFormat::VK_FORMAT_R8G8B8A8_USCALED => unsupported,
            VkFormat::VK_FORMAT_R8G8B8A8_SSCALED => unsupported,
            VkFormat::VK_FORMAT_R8G8B8A8_UINT => unsupported,
            VkFormat::VK_FORMAT_R8G8B8A8_SINT => unsupported,
            VkFormat::VK_FORMAT_R8G8B8A8_SRGB => unsupported,
            VkFormat::VK_FORMAT_B8G8R8A8_UNORM => unsupported,
            VkFormat::VK_FORMAT_B8G8R8A8_SNORM => unsupported,
            VkFormat::VK_FORMAT_B8G8R8A8_USCALED => unsupported,
            VkFormat::VK_FORMAT_B8G8R8A8_SSCALED => unsupported,
            VkFormat::VK_FORMAT_B8G8R8A8_UINT => unsupported,
            VkFormat::VK_FORMAT_B8G8R8A8_SINT => unsupported,
            VkFormat::VK_FORMAT_B8G8R8A8_SRGB => unsupported,
            VkFormat::VK_FORMAT_A8B8G8R8_UNORM_PACK32 => unsupported,
            VkFormat::VK_FORMAT_A8B8G8R8_SNORM_PACK32 => unsupported,
            VkFormat::VK_FORMAT_A8B8G8R8_USCALED_PACK32 => unsupported,
            VkFormat::VK_FORMAT_A8B8G8R8_SSCALED_PACK32 => unsupported,
            VkFormat::VK_FORMAT_A8B8G8R8_UINT_PACK32 => unsupported,
            VkFormat::VK_FORMAT_A8B8G8R8_SINT_PACK32 => unsupported,
            VkFormat::VK_FORMAT_A8B8G8R8_SRGB_PACK32 => unsupported,
            VkFormat::VK_FORMAT_A2R10G10B10_UNORM_PACK32 => unsupported,
            VkFormat::VK_FORMAT_A2R10G10B10_SNORM_PACK32 => unsupported,
            VkFormat::VK_FORMAT_A2R10G10B10_USCALED_PACK32 => unsupported,
            VkFormat::VK_FORMAT_A2R10G10B10_SSCALED_PACK32 => unsupported,
            VkFormat::VK_FORMAT_A2R10G10B10_UINT_PACK32 => unsupported,
            VkFormat::VK_FORMAT_A2R10G10B10_SINT_PACK32 => unsupported,
            VkFormat::VK_FORMAT_A2B10G10R10_UNORM_PACK32 => unsupported,
            VkFormat::VK_FORMAT_A2B10G10R10_SNORM_PACK32 => unsupported,
            VkFormat::VK_FORMAT_A2B10G10R10_USCALED_PACK32 => unsupported,
            VkFormat::VK_FORMAT_A2B10G10R10_SSCALED_PACK32 => unsupported,
            VkFormat::VK_FORMAT_A2B10G10R10_UINT_PACK32 => unsupported,
            VkFormat::VK_FORMAT_A2B10G10R10_SINT_PACK32 => unsupported,
            VkFormat::VK_FORMAT_R16_UNORM => unsupported,
            VkFormat::VK_FORMAT_R16_SNORM => unsupported,
            VkFormat::VK_FORMAT_R16_USCALED => unsupported,
            VkFormat::VK_FORMAT_R16_SSCALED => unsupported,
            VkFormat::VK_FORMAT_R16_UINT => unsupported,
            VkFormat::VK_FORMAT_R16_SINT => unsupported,
            VkFormat::VK_FORMAT_R16_SFLOAT => unsupported,
            VkFormat::VK_FORMAT_R16G16_UNORM => unsupported,
            VkFormat::VK_FORMAT_R16G16_SNORM => unsupported,
            VkFormat::VK_FORMAT_R16G16_USCALED => unsupported,
            VkFormat::VK_FORMAT_R16G16_SSCALED => unsupported,
            VkFormat::VK_FORMAT_R16G16_UINT => unsupported,
            VkFormat::VK_FORMAT_R16G16_SINT => unsupported,
            VkFormat::VK_FORMAT_R16G16_SFLOAT => unsupported,
            VkFormat::VK_FORMAT_R16G16B16_UNORM => unsupported,
            VkFormat::VK_FORMAT_R16G16B16_SNORM => unsupported,
            VkFormat::VK_FORMAT_R16G16B16_USCALED => unsupported,
            VkFormat::VK_FORMAT_R16G16B16_SSCALED => unsupported,
            VkFormat::VK_FORMAT_R16G16B16_UINT => unsupported,
            VkFormat::VK_FORMAT_R16G16B16_SINT => unsupported,
            VkFormat::VK_FORMAT_R16G16B16_SFLOAT => unsupported,
            VkFormat::VK_FORMAT_R16G16B16A16_UNORM => unsupported,
            VkFormat::VK_FORMAT_R16G16B16A16_SNORM => unsupported,
            VkFormat::VK_FORMAT_R16G16B16A16_USCALED => unsupported,
            VkFormat::VK_FORMAT_R16G16B16A16_SSCALED => unsupported,
            VkFormat::VK_FORMAT_R16G16B16A16_UINT => unsupported,
            VkFormat::VK_FORMAT_R16G16B16A16_SINT => unsupported,
            VkFormat::VK_FORMAT_R16G16B16A16_SFLOAT => unsupported,
            VkFormat::VK_FORMAT_R32_UINT => unsupported,
            VkFormat::VK_FORMAT_R32_SINT => unsupported,
            VkFormat::VK_FORMAT_R32_SFLOAT => unsupported,
            VkFormat::VK_FORMAT_R32G32_UINT => unsupported,
            VkFormat::VK_FORMAT_R32G32_SINT => unsupported,
            VkFormat::VK_FORMAT_R32G32_SFLOAT => unsupported,
            VkFormat::VK_FORMAT_R32G32B32_UINT => unsupported,
            VkFormat::VK_FORMAT_R32G32B32_SINT => unsupported,
            VkFormat::VK_FORMAT_R32G32B32_SFLOAT => unsupported,
            VkFormat::VK_FORMAT_R32G32B32A32_UINT => unsupported,
            VkFormat::VK_FORMAT_R32G32B32A32_SINT => unsupported,
            VkFormat::VK_FORMAT_R32G32B32A32_SFLOAT => unsupported,
            VkFormat::VK_FORMAT_R64_UINT => unsupported,
            VkFormat::VK_FORMAT_R64_SINT => unsupported,
            VkFormat::VK_FORMAT_R64_SFLOAT => unsupported,
            VkFormat::VK_FORMAT_R64G64_UINT => unsupported,
            VkFormat::VK_FORMAT_R64G64_SINT => unsupported,
            VkFormat::VK_FORMAT_R64G64_SFLOAT => unsupported,
            VkFormat::VK_FORMAT_R64G64B64_UINT => unsupported,
            VkFormat::VK_FORMAT_R64G64B64_SINT => unsupported,
            VkFormat::VK_FORMAT_R64G64B64_SFLOAT => unsupported,
            VkFormat::VK_FORMAT_R64G64B64A64_UINT => unsupported,
            VkFormat::VK_FORMAT_R64G64B64A64_SINT => unsupported,
            VkFormat::VK_FORMAT_R64G64B64A64_SFLOAT => unsupported,
            VkFormat::VK_FORMAT_B10G11R11_UFLOAT_PACK32 => unsupported,
            VkFormat::VK_FORMAT_E5B9G9R9_UFLOAT_PACK32 => unsupported,
            VkFormat::VK_FORMAT_D16_UNORM => unsupported,
            VkFormat::VK_FORMAT_X8_D24_UNORM_PACK32 => unsupported,
            VkFormat::VK_FORMAT_D32_SFLOAT => unsupported,
            VkFormat::VK_FORMAT_S8_UINT => unsupported,
            VkFormat::VK_FORMAT_D16_UNORM_S8_UINT => unsupported,
            VkFormat::VK_FORMAT_D24_UNORM_S8_UINT => unsupported,
            VkFormat::VK_FORMAT_D32_SFLOAT_S8_UINT => unsupported,
            VkFormat::VK_FORMAT_BC1_RGB_UNORM_BLOCK => unsupported,
            VkFormat::VK_FORMAT_BC1_RGB_SRGB_BLOCK => unsupported,
            VkFormat::VK_FORMAT_BC1_RGBA_UNORM_BLOCK => unsupported,
            VkFormat::VK_FORMAT_BC1_RGBA_SRGB_BLOCK => unsupported,
            VkFormat::VK_FORMAT_BC2_UNORM_BLOCK => unsupported,
            VkFormat::VK_FORMAT_BC2_SRGB_BLOCK => unsupported,
            VkFormat::VK_FORMAT_BC3_UNORM_BLOCK => unsupported,
            VkFormat::VK_FORMAT_BC3_SRGB_BLOCK => unsupported,
            VkFormat::VK_FORMAT_BC4_UNORM_BLOCK => unsupported,
            VkFormat::VK_FORMAT_BC4_SNORM_BLOCK => unsupported,
            VkFormat::VK_FORMAT_BC5_UNORM_BLOCK => unsupported,
            VkFormat::VK_FORMAT_BC5_SNORM_BLOCK => unsupported,
            VkFormat::VK_FORMAT_BC6H_UFLOAT_BLOCK => unsupported,
            VkFormat::VK_FORMAT_BC6H_SFLOAT_BLOCK => unsupported,
            VkFormat::VK_FORMAT_BC7_UNORM_BLOCK => unsupported,
            VkFormat::VK_FORMAT_BC7_SRGB_BLOCK => unsupported,
            VkFormat::VK_FORMAT_ETC2_R8G8B8_UNORM_BLOCK => unsupported,
            VkFormat::VK_FORMAT_ETC2_R8G8B8_SRGB_BLOCK => unsupported,
            VkFormat::VK_FORMAT_ETC2_R8G8B8A1_UNORM_BLOCK => unsupported,
            VkFormat::VK_FORMAT_ETC2_R8G8B8A1_SRGB_BLOCK => unsupported,
            VkFormat::VK_FORMAT_ETC2_R8G8B8A8_UNORM_BLOCK => unsupported,
            VkFormat::VK_FORMAT_ETC2_R8G8B8A8_SRGB_BLOCK => unsupported,
            VkFormat::VK_FORMAT_EAC_R11_UNORM_BLOCK => unsupported,
            VkFormat::VK_FORMAT_EAC_R11_SNORM_BLOCK => unsupported,
            VkFormat::VK_FORMAT_EAC_R11G11_UNORM_BLOCK => unsupported,
            VkFormat::VK_FORMAT_EAC_R11G11_SNORM_BLOCK => unsupported,
            VkFormat::VK_FORMAT_ASTC_4x4_UNORM_BLOCK => unsupported,
            VkFormat::VK_FORMAT_ASTC_4x4_SRGB_BLOCK => unsupported,
            VkFormat::VK_FORMAT_ASTC_5x4_UNORM_BLOCK => unsupported,
            VkFormat::VK_FORMAT_ASTC_5x4_SRGB_BLOCK => unsupported,
            VkFormat::VK_FORMAT_ASTC_5x5_UNORM_BLOCK => unsupported,
            VkFormat::VK_FORMAT_ASTC_5x5_SRGB_BLOCK => unsupported,
            VkFormat::VK_FORMAT_ASTC_6x5_UNORM_BLOCK => unsupported,
            VkFormat::VK_FORMAT_ASTC_6x5_SRGB_BLOCK => unsupported,
            VkFormat::VK_FORMAT_ASTC_6x6_UNORM_BLOCK => unsupported,
            VkFormat::VK_FORMAT_ASTC_6x6_SRGB_BLOCK => unsupported,
            VkFormat::VK_FORMAT_ASTC_8x5_UNORM_BLOCK => unsupported,
            VkFormat::VK_FORMAT_ASTC_8x5_SRGB_BLOCK => unsupported,
            VkFormat::VK_FORMAT_ASTC_8x6_UNORM_BLOCK => unsupported,
            VkFormat::VK_FORMAT_ASTC_8x6_SRGB_BLOCK => unsupported,
            VkFormat::VK_FORMAT_ASTC_8x8_UNORM_BLOCK => unsupported,
            VkFormat::VK_FORMAT_ASTC_8x8_SRGB_BLOCK => unsupported,
            VkFormat::VK_FORMAT_ASTC_10x5_UNORM_BLOCK => unsupported,
            VkFormat::VK_FORMAT_ASTC_10x5_SRGB_BLOCK => unsupported,
            VkFormat::VK_FORMAT_ASTC_10x6_UNORM_BLOCK => unsupported,
            VkFormat::VK_FORMAT_ASTC_10x6_SRGB_BLOCK => unsupported,
            VkFormat::VK_FORMAT_ASTC_10x8_UNORM_BLOCK => unsupported,
            VkFormat::VK_FORMAT_ASTC_10x8_SRGB_BLOCK => unsupported,
            VkFormat::VK_FORMAT_ASTC_10x10_UNORM_BLOCK => unsupported,
            VkFormat::VK_FORMAT_ASTC_10x10_SRGB_BLOCK => unsupported,
            VkFormat::VK_FORMAT_ASTC_12x10_UNORM_BLOCK => unsupported,
            VkFormat::VK_FORMAT_ASTC_12x10_SRGB_BLOCK => unsupported,
            VkFormat::VK_FORMAT_ASTC_12x12_UNORM_BLOCK => unsupported,
            VkFormat::VK_FORMAT_ASTC_12x12_SRGB_BLOCK => unsupported,
            VkFormat(185_u32..=u32::MAX) => unreachable!(),
        }
    }

    pub fn queue_family_properties(&self) -> [VkQueueFamilyProperties; 1] {
        // SPEC: If an implementation exposes any queue family that supports graphics operations,
        // at least one queue family of at least one physical device exposed by the implementation
        // must support both graphics and compute operations.
        let graphics_queue_family_properties = VkQueueFamilyProperties {
            queueFlags: (VkQueueFlagBits::VK_QUEUE_GRAPHICS_BIT
                | VkQueueFlagBits::VK_QUEUE_COMPUTE_BIT)
                .into(),
            queueCount: 1,
            timestampValidBits: 0,
            minImageTransferGranularity: VkExtent3D {
                width: 0,
                height: 0,
                depth: 0,
            },
        };
        [graphics_queue_family_properties]
    }

    pub const fn surface_support(&self, queue_family_index: u32, _surface: VkSurfaceKHR) -> bool {
        queue_family_index == 0
    }

    pub const fn present_modes(&self) -> [VkPresentModeKHR; 1] {
        [VkPresentModeKHR::VK_PRESENT_MODE_FIFO_KHR]
    }

    pub const fn surface_formats(&self) -> [VkSurfaceFormatKHR; 2] {
        [
            VkSurfaceFormatKHR {
                format: VkFormat::VK_FORMAT_R8G8B8A8_UNORM,
                colorSpace: VkColorSpaceKHR::VK_COLOR_SPACE_SRGB_NONLINEAR_KHR,
            },
            VkSurfaceFormatKHR {
                format: VkFormat::VK_FORMAT_R8G8B8A8_SRGB,
                colorSpace: VkColorSpaceKHR::VK_COLOR_SPACE_SRGB_NONLINEAR_KHR,
            },
        ]
    }

    pub fn surface_capabilities(&self) -> VkSurfaceCapabilitiesKHR {
        VkSurfaceCapabilitiesKHR {
            minImageCount: 1,
            maxImageCount: 2,
            currentExtent: VkExtent2D {
                width: 0xFFFFFFFF,
                height: 0xFFFFFFFF,
            },
            minImageExtent: VkExtent2D {
                width: 0,
                height: 0,
            },
            maxImageExtent: VkExtent2D {
                width: 16384, // TODO: Replace with maxImageDimension2D.
                height: 16384,
            },
            maxImageArrayLayers: 1,
            supportedTransforms:
                VkSurfaceTransformFlagBitsKHR::VK_SURFACE_TRANSFORM_IDENTITY_BIT_KHR.into(),
            currentTransform: VkSurfaceTransformFlagBitsKHR::VK_SURFACE_TRANSFORM_IDENTITY_BIT_KHR,
            supportedCompositeAlpha: VkCompositeAlphaFlagBitsKHR::VK_COMPOSITE_ALPHA_OPAQUE_BIT_KHR
                .into(),
            supportedUsageFlags: VkImageUsageFlagBits::VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT.into(),
        }
    }
}

impl Dispatchable for PhysicalDevice {
    fn get_vec<'a>(&'a self, context: &'a mut RwLockWriteGuard<Context>) -> &mut Vec<Arc<Self>> {
        context.physical_devices.as_mut()
    }
}

/* LogicalDevice */

/// Identifier used to associate functions with a `PhysicalDevice`.
#[derive(Debug)]
pub struct LogicalDevice {
    driver_name: &'static str,
    physical_device: Arc<PhysicalDevice>,
    queue: Arc<Queue>,
}

impl LogicalDevice {
    pub fn new(
        physical_device: &Arc<PhysicalDevice>,
        queue_create_info: &VkDeviceQueueCreateInfo,
    ) -> Arc<Self> {
        info!("new LogicalDevice");
        let physical_device = physical_device.clone();

        let queue = Queue::new(physical_device.clone(), queue_create_info);

        let logical_device = Self {
            driver_name: "vulkan_software_rasterizer",
            physical_device,
            queue,
        };
        let logical_device = Arc::new(logical_device);
        logical_device.register_object()
    }
}

impl Dispatchable for LogicalDevice {
    fn get_vec<'a>(&'a self, context: &'a mut RwLockWriteGuard<Context>) -> &mut Vec<Arc<Self>> {
        context.logical_devices.as_mut()
    }
}

impl LogicalDevice {
    pub fn queue(&self, queue_family_index: u32, queue_index: u32) -> Arc<Queue> {
        let _ = queue_family_index;
        let _ = queue_index;
        self.queue.clone()
    }

    pub fn wait_for_fences(&self, fences: Vec<Arc<Mutex<Fence>>>, wait_all: bool, timeout: u64) {
        let _ = wait_all;
        let _ = timeout;
        for fence in fences {
            if fence.lock().logical_device.as_ref() as *const _ != self as *const _ {
                continue;
            }
            // TODO: Wait for one or more fences to become signaled.
        }
    }

    pub fn reset_fences(&self, fences: Vec<Arc<Mutex<Fence>>>) {
        for mut fence in fences {
            // TODO: VUID-vkResetFences-pFences-01123
            fence.lock().reset();
        }
    }
}

/* Queue */

/// Queue associated with `LogicalDevice`.
#[derive(Debug)]
pub struct Queue {
    physical_device: Arc<PhysicalDevice>,
    flags: VkDeviceQueueCreateFlags,
}

impl Queue {
    pub fn new(
        physical_device: Arc<PhysicalDevice>,
        create_info: &VkDeviceQueueCreateInfo,
    ) -> Arc<Self> {
        info!("new Queue");
        let flags = create_info.flags;

        let queue = Self {
            physical_device,
            flags,
        };
        let queue = Arc::new(queue);
        queue.register_object()
    }
}

impl Dispatchable for Queue {
    fn get_vec<'a>(&'a self, context: &'a mut RwLockWriteGuard<Context>) -> &mut Vec<Arc<Self>> {
        context.queues.as_mut()
    }
}

/* Fence */

/// Synchronization primitive that can be used to insert a dependency from a queue to the host.
#[derive(Debug)]
pub struct Fence {
    handle: VkNonDispatchableHandle,
    logical_device: Arc<LogicalDevice>,
    flags: VkFenceCreateFlags,
    signaled: bool,
}

impl Fence {
    pub fn create(
        logical_device: Arc<LogicalDevice>,
        create_info: &VkFenceCreateInfo,
    ) -> VkNonDispatchableHandle {
        info!("new Fence");
        let handle = VK_NULL_HANDLE;
        let flags = create_info.flags;
        let signaled = (Into::<VkFenceCreateFlagBits>::into(flags)
            & VkFenceCreateFlagBits::VK_FENCE_CREATE_SIGNALED_BIT)
            != 0;
        let fence = Self {
            handle,
            logical_device,
            flags,
            signaled,
        };
        fence.register_object()
    }

    pub fn signal(&mut self) {
        trace!("fence {} signal", self.signaled);
        self.signaled = true;
    }

    pub fn reset(&mut self) {
        trace!("fence {} reset", self.signaled);
        self.signaled = false;
    }
}

impl NonDispatchable for Fence {
    fn get_hash<'a>(
        context: &'a Context,
    ) -> &'a HashMap<VkNonDispatchableHandle, Arc<Mutex<Self>>> {
        &context.fences
    }

    fn get_hash_mut(
        context: &mut Context,
    ) -> &mut HashMap<VkNonDispatchableHandle, Arc<Mutex<Self>>> {
        &mut context.fences
    }

    fn set_handle(&mut self, handle: VkNonDispatchableHandle) {
        self.handle = handle;
    }

    fn get_handle(&self) -> VkNonDispatchableHandle {
        self.handle
    }
}

/* Semaphore */

/// Synchronization primitive that can be used to insert a dependency between queue operations or
/// between a queue operation and the host.
#[derive(Debug)]
pub struct Semaphore {
    handle: VkNonDispatchableHandle,
    flags: VkSemaphoreCreateFlags,
}

impl Semaphore {
    pub fn create(create_info: &VkSemaphoreCreateInfo) -> VkNonDispatchableHandle {
        info!("new Semaphore");
        let handle = VK_NULL_HANDLE;
        let flags = create_info.flags;

        let semaphore = Self { handle, flags };
        semaphore.register_object()
    }
}

impl NonDispatchable for Semaphore {
    fn get_hash<'a>(
        context: &'a Context,
    ) -> &'a HashMap<VkNonDispatchableHandle, Arc<Mutex<Self>>> {
        &context.semaphores
    }

    fn get_hash_mut<'a>(
        context: &'a mut Context,
    ) -> &'a mut HashMap<VkNonDispatchableHandle, Arc<Mutex<Self>>> {
        &mut context.semaphores
    }

    fn set_handle(&mut self, handle: VkNonDispatchableHandle) {
        self.handle = handle;
    }

    fn get_handle(&self) -> VkNonDispatchableHandle {
        self.handle
    }
}
