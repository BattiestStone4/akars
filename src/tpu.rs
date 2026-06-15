use crate::camera::CameraFrame;
use crate::detector::Detection;
use std::error::Error;
use std::fmt;
use std::path::Path;

#[derive(Clone, Copy, Debug)]
pub struct InferenceConfig {
    pub classes_num: i32,
    pub confidence_threshold: f32,
    pub iou_threshold: f32,
}

impl Default for InferenceConfig {
    fn default() -> Self {
        Self {
            classes_num: 1,
            confidence_threshold: 0.5,
            iou_threshold: 0.5,
        }
    }
}

#[derive(Debug)]
pub struct TpuError(String);

impl TpuError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for TpuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for TpuError {}

#[cfg(akars_sg2002)]
mod imp {
    use super::{CameraFrame, Detection, InferenceConfig, TpuError};
    use crate::detector::{correct_yolo_boxes, nms, parse_yolov8_output};
    use std::ffi::{c_char, c_int, c_void, CString};
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;
    use std::ptr;
    use std::slice;

    const CVI_FMT_FP32: i32 = 0;
    const CVI_FMT_BF16: i32 = 3;
    const CVI_FMT_INT16: i32 = 4;
    const CVI_FMT_INT8: i32 = 6;
    const CVI_FMT_UINT8: i32 = 7;
    const CVI_RC_SUCCESS: i32 = 0;
    const CVI_DIM_MAX: usize = 6;

    #[repr(C)]
    #[derive(Clone, Copy, Debug)]
    struct CviShape {
        dim: [i32; CVI_DIM_MAX],
        dim_size: usize,
    }

    #[repr(C)]
    #[derive(Debug)]
    struct CviTensor {
        name: *mut c_char,
        shape: CviShape,
        fmt: i32,
        count: usize,
        mem_size: usize,
        sys_mem: *mut u8,
        paddr: u64,
        mem_type: i32,
        qscale: f32,
        zero_point: c_int,
        pixel_format: i32,
        aligned: bool,
        mean: [f32; 3],
        scale: [f32; 3],
        owner: *mut c_void,
        reserved: [c_char; 32],
    }

    type CviModelHandle = *mut c_void;

    unsafe extern "C" {
        fn CVI_NN_RegisterModel(model_file: *const c_char, model: *mut CviModelHandle) -> i32;
        fn CVI_NN_GetInputOutputTensors(
            model: CviModelHandle,
            inputs: *mut *mut CviTensor,
            input_num: *mut i32,
            outputs: *mut *mut CviTensor,
            output_num: *mut i32,
        ) -> i32;
        fn CVI_NN_GetTensorByName(
            name: *const c_char,
            tensors: *mut CviTensor,
            num: i32,
        ) -> *mut CviTensor;
        fn CVI_NN_TensorPtr(tensor: *mut CviTensor) -> *mut c_void;
        fn CVI_NN_TensorShape(tensor: *mut CviTensor) -> CviShape;
        fn CVI_NN_Forward(
            model: CviModelHandle,
            inputs: *mut CviTensor,
            input_num: i32,
            outputs: *mut CviTensor,
            output_num: i32,
        ) -> i32;
        fn CVI_NN_CleanupModel(model: CviModelHandle) -> i32;

        fn akars_mjpeg_to_rgb_planar(
            jpeg: *const u8,
            jpeg_len: usize,
            dst: *mut u8,
            dst_w: i32,
            dst_h: i32,
            src_w: *mut i32,
            src_h: *mut i32,
        ) -> i32;
    }

    pub struct YoloModel {
        model: CviModelHandle,
        inputs: *mut CviTensor,
        input_num: i32,
        outputs: *mut CviTensor,
        output_num: i32,
        input: *mut CviTensor,
        input_h: i32,
        input_w: i32,
        output_shapes: Vec<CviShape>,
    }

    impl YoloModel {
        pub fn open(path: &Path) -> Result<Self, TpuError> {
            let c_path = CString::new(path.as_os_str().as_bytes())
                .map_err(|_| TpuError::new("model path contains NUL byte"))?;
            let mut model: CviModelHandle = ptr::null_mut();
            let rc = unsafe { CVI_NN_RegisterModel(c_path.as_ptr(), &mut model) };
            if rc != CVI_RC_SUCCESS {
                return Err(TpuError::new(format!("CVI_NN_RegisterModel failed: {rc}")));
            }

            let mut inputs = ptr::null_mut();
            let mut outputs = ptr::null_mut();
            let mut input_num = 0;
            let mut output_num = 0;
            let rc = unsafe {
                CVI_NN_GetInputOutputTensors(
                    model,
                    &mut inputs,
                    &mut input_num,
                    &mut outputs,
                    &mut output_num,
                )
            };
            if rc != CVI_RC_SUCCESS {
                unsafe {
                    CVI_NN_CleanupModel(model);
                }
                return Err(TpuError::new(format!(
                    "CVI_NN_GetInputOutputTensors failed: {rc}"
                )));
            }

            let input = unsafe { CVI_NN_GetTensorByName(ptr::null(), inputs, input_num) };
            if input.is_null() {
                unsafe {
                    CVI_NN_CleanupModel(model);
                }
                return Err(TpuError::new("default input tensor not found"));
            }

            let input_shape = unsafe { CVI_NN_TensorShape(input) };
            let input_h = input_shape.dim[2];
            let input_w = input_shape.dim[3];
            let outputs_slice = unsafe { slice::from_raw_parts_mut(outputs, output_num as usize) };
            let output_shapes = outputs_slice
                .iter_mut()
                .map(|tensor| unsafe { CVI_NN_TensorShape(tensor as *mut CviTensor) })
                .collect();

            Ok(Self {
                model,
                inputs,
                input_num,
                outputs,
                output_num,
                input,
                input_h,
                input_w,
                output_shapes,
            })
        }

        pub fn infer(
            &mut self,
            frame: &CameraFrame,
            config: InferenceConfig,
        ) -> Result<Vec<Detection>, TpuError> {
            let input_ptr = unsafe { CVI_NN_TensorPtr(self.input) as *mut u8 };
            if input_ptr.is_null() {
                return Err(TpuError::new("input tensor pointer is null"));
            }

            let mut decoded_w = 0;
            let mut decoded_h = 0;
            let rc = unsafe {
                akars_mjpeg_to_rgb_planar(
                    frame.jpeg.as_ptr(),
                    frame.jpeg.len(),
                    input_ptr,
                    self.input_w,
                    self.input_h,
                    &mut decoded_w,
                    &mut decoded_h,
                )
            };
            if rc != 0 {
                return Err(TpuError::new(format!(
                    "MJPEG decode/preprocess failed: {rc}"
                )));
            }

            let rc = unsafe {
                CVI_NN_Forward(
                    self.model,
                    self.inputs,
                    self.input_num,
                    self.outputs,
                    self.output_num,
                )
            };
            if rc != CVI_RC_SUCCESS {
                return Err(TpuError::new(format!("CVI_NN_Forward failed: {rc}")));
            }

            let mut detections = self.get_detections(config)?;
            nms(&mut detections, config.iou_threshold);

            let image_w = if decoded_w > 0 {
                decoded_w
            } else {
                frame.width as i32
            };
            let image_h = if decoded_h > 0 {
                decoded_h
            } else {
                frame.height as i32
            };
            correct_yolo_boxes(
                &mut detections,
                image_h,
                image_w,
                self.input_h,
                self.input_w,
            );
            Ok(detections)
        }

        fn get_detections(&mut self, config: InferenceConfig) -> Result<Vec<Detection>, TpuError> {
            if self.output_num < 1 || self.output_shapes.is_empty() {
                return Err(TpuError::new("model has no output tensor"));
            }
            let output = unsafe { &mut *self.outputs };
            let shape = self.output_shapes[0];
            let count = output.count;
            let ptr = unsafe { CVI_NN_TensorPtr(output as *mut CviTensor) };
            if ptr.is_null() {
                return Err(TpuError::new("output tensor pointer is null"));
            }

            let data = tensor_to_f32(output, ptr, count)?;
            Ok(parse_yolov8_output(
                &data,
                [shape.dim[0], shape.dim[1], shape.dim[2], shape.dim[3]],
                config.classes_num,
                config.confidence_threshold,
            ))
        }
    }

    impl Drop for YoloModel {
        fn drop(&mut self) {
            if !self.model.is_null() {
                unsafe {
                    CVI_NN_CleanupModel(self.model);
                }
            }
        }
    }

    fn tensor_to_f32(
        tensor: &CviTensor,
        ptr: *mut c_void,
        count: usize,
    ) -> Result<Vec<f32>, TpuError> {
        match tensor.fmt {
            CVI_FMT_FP32 => {
                let src = unsafe { slice::from_raw_parts(ptr as *const f32, count) };
                Ok(src.to_vec())
            }
            CVI_FMT_INT8 => {
                let src = unsafe { slice::from_raw_parts(ptr as *const i8, count) };
                Ok(src.iter().map(|v| *v as f32 * tensor.qscale).collect())
            }
            CVI_FMT_UINT8 => {
                let src = unsafe { slice::from_raw_parts(ptr as *const u8, count) };
                Ok(src
                    .iter()
                    .map(|v| (*v as i32 - tensor.zero_point) as f32 * tensor.qscale)
                    .collect())
            }
            CVI_FMT_BF16 => {
                let src = unsafe { slice::from_raw_parts(ptr as *const u16, count) };
                Ok(src
                    .iter()
                    .map(|v| f32::from_bits((*v as u32) << 16))
                    .collect())
            }
            CVI_FMT_INT16 => {
                let src = unsafe { slice::from_raw_parts(ptr as *const i16, count) };
                Ok(src.iter().map(|v| *v as f32 * tensor.qscale).collect())
            }
            other => Err(TpuError::new(format!(
                "unsupported output tensor format: {other}"
            ))),
        }
    }
}

#[cfg(not(akars_sg2002))]
mod imp {
    use super::{CameraFrame, Detection, InferenceConfig, TpuError};
    use std::path::Path;

    pub struct YoloModel;

    impl YoloModel {
        pub fn open(_path: &Path) -> Result<Self, TpuError> {
            Err(TpuError::new(
                "akars was built without SG2002 TPU/OpenCV runtime support",
            ))
        }

        pub fn infer(
            &mut self,
            _frame: &CameraFrame,
            _config: InferenceConfig,
        ) -> Result<Vec<Detection>, TpuError> {
            Err(TpuError::new(
                "akars was built without SG2002 TPU/OpenCV runtime support",
            ))
        }
    }
}

pub use imp::YoloModel;

pub fn open_model(path: impl AsRef<Path>) -> Result<YoloModel, TpuError> {
    YoloModel::open(path.as_ref())
}
