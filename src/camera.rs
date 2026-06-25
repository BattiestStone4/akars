use crate::linux;
use std::fs::File;
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

const CVI_CAMERA_IOCTL_INIT: u64 = 1;
const CVI_CAMERA_IOCTL_GET_INFO: u64 = 2;
const CVI_CAMERA_IOCTL_GET_FRAME: u64 = 3;
const FRAME_BUFFER_SIZE: usize = 2 * 1024 * 1024;
const JPEG_MARKER_START: [u8; 2] = [0xFF, 0xD8];
const JPEG_MARKER_END: [u8; 2] = [0xFF, 0xD9];

#[repr(C, packed)]
#[derive(Clone, Copy, Default)]
struct RawCameraInfo {
    width: u16,
    height: u16,
    format: u8,
    connected: u8,
}

#[derive(Clone, Copy, Debug)]
pub struct CameraInfo {
    pub width: u16,
    pub height: u16,
    pub format: u8,
    pub connected: bool,
}

impl RawCameraInfo {
    fn unpack(&self) -> CameraInfo {
        let width = unsafe { std::ptr::addr_of!(self.width).read_unaligned() };
        let height = unsafe { std::ptr::addr_of!(self.height).read_unaligned() };
        CameraInfo {
            width,
            height,
            format: self.format,
            connected: self.connected != 0,
        }
    }
}

#[derive(Debug)]
pub struct CameraFrame {
    pub jpeg: Vec<u8>,
    pub width: u16,
    pub height: u16,
}

pub struct UsbCamera {
    file: File,
    info: CameraInfo,
}

impl UsbCamera {
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        // StarryOS open 不支持 O_NOCTTY + write(false 避免 EISDIR)
        let file = std::fs::OpenOptions::new()
            .read(true)
            .open(path.as_ref())?;

        ioctl_no_arg(file.as_raw_fd(), CVI_CAMERA_IOCTL_INIT)?;

        let mut raw = RawCameraInfo::default();
        ioctl_ptr(
            file.as_raw_fd(),
            CVI_CAMERA_IOCTL_GET_INFO,
            &mut raw as *mut RawCameraInfo,
        )?;
        let info = raw.unpack();
        if !info.connected {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "camera reports disconnected",
            ));
        }
        if info.format != 1 {
            eprintln!(
                "[camera] warning: camera format {} is not MJPEG(1)",
                info.format
            );
        }

        Ok(Self { file, info })
    }

    pub fn info(&self) -> CameraInfo {
        self.info
    }

    pub fn get_frame(&mut self) -> io::Result<CameraFrame> {
        let mut buffer = vec![0u8; FRAME_BUFFER_SIZE];
        let ret = unsafe {
            linux::ioctl(
                self.file.as_raw_fd(),
                CVI_CAMERA_IOCTL_GET_FRAME as _,
                buffer.as_mut_ptr(),
            )
        };
        if ret < 0 {
            return Err(io::Error::last_os_error());
        }

        let size = ret as usize;
        buffer.truncate(size);
        if !is_valid_jpeg(&buffer) {
            eprintln!("[camera] warning: captured frame is not a complete JPEG");
        }

        Ok(CameraFrame {
            jpeg: buffer,
            width: self.info.width,
            height: self.info.height,
        })
    }
}

fn ioctl_no_arg(fd: i32, request: u64) -> io::Result<()> {
    let ret = unsafe { linux::ioctl(fd, request as _, 0usize) };
    if ret < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn ioctl_ptr<T>(fd: i32, request: u64, ptr: *mut T) -> io::Result<()> {
    let ret = unsafe { linux::ioctl(fd, request as _, ptr) };
    if ret < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub fn is_valid_jpeg(data: &[u8]) -> bool {
    data.len() >= 4 && data.starts_with(&JPEG_MARKER_START) && data.ends_with(&JPEG_MARKER_END)
}

#[cfg(test)]
mod tests {
    use super::is_valid_jpeg;

    #[test]
    fn checks_jpeg_markers() {
        assert!(is_valid_jpeg(&[0xFF, 0xD8, 1, 2, 0xFF, 0xD9]));
        assert!(!is_valid_jpeg(&[0x00, 0xD8, 1, 2, 0xFF, 0xD9]));
        assert!(!is_valid_jpeg(&[0xFF, 0xD8, 1]));
    }
}
