//! Reading and writing another process's memory (Windows).

use std::path::PathBuf;
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
    System::{
        Diagnostics::{
            Debug::{ReadProcessMemory, WriteProcessMemory},
            ToolHelp::{CreateToolhelp32Snapshot, Module32FirstW, Module32NextW, MODULEENTRY32W, TH32CS_SNAPMODULE, TH32CS_SNAPMODULE32},
        },
        Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_OPERATION, PROCESS_VM_READ, PROCESS_VM_WRITE},
    },
};

pub struct Proc {
    handle: HANDLE,
    pub pid: u32,
}

// The handle is only used through ReadProcessMemory/WriteProcessMemory, which are thread-safe.
unsafe impl Send for Proc {}
unsafe impl Sync for Proc {}

impl Proc {
    pub fn open(pid: u32) -> Result<Proc, String> {
        let access = PROCESS_VM_READ | PROCESS_VM_WRITE | PROCESS_VM_OPERATION | PROCESS_QUERY_INFORMATION;
        let handle = unsafe { OpenProcess(access, 0, pid) };
        if handle.is_null() {
            return Err(format!("Couldn't open process {pid} (error {})", std::io::Error::last_os_error()));
        }
        Ok(Proc { handle, pid })
    }

    pub fn read_into(&self, addr: u64, buf: &mut [u8]) -> bool {
        if addr == 0 {
            return false;
        }
        let mut got = 0usize;
        let ok = unsafe { ReadProcessMemory(self.handle, addr as _, buf.as_mut_ptr() as _, buf.len(), &mut got) };
        ok != 0 && got == buf.len()
    }

    pub fn read<T: Copy + Default>(&self, addr: u64) -> Option<T> {
        let mut v = T::default();
        let buf = unsafe { std::slice::from_raw_parts_mut(&mut v as *mut T as *mut u8, std::mem::size_of::<T>()) };
        self.read_into(addr, buf).then_some(v)
    }

    pub fn ptr(&self, addr: u64) -> Option<u64> {
        self.read::<u64>(addr).filter(|&p| p != 0)
    }

    pub fn write<T: Copy>(&self, addr: u64, v: T) -> bool {
        let mut put = 0usize;
        let ok = unsafe { WriteProcessMemory(self.handle, addr as _, &v as *const T as _, std::mem::size_of::<T>(), &mut put) };
        ok != 0 && put == std::mem::size_of::<T>()
    }

    /// Base address and file path of the first loaded module whose file name contains `needle`.
    pub fn module(&self, needle: &str) -> Option<(u64, PathBuf)> {
        let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, self.pid) };
        if snap == INVALID_HANDLE_VALUE {
            return None;
        }
        let mut entry: MODULEENTRY32W = unsafe { std::mem::zeroed() };
        entry.dwSize = std::mem::size_of::<MODULEENTRY32W>() as u32;
        let mut found = None;
        let mut ok = unsafe { Module32FirstW(snap, &mut entry) };
        while ok != 0 {
            let len = entry.szExePath.iter().position(|&c| c == 0).unwrap_or(entry.szExePath.len());
            let path = PathBuf::from(String::from_utf16_lossy(&entry.szExePath[..len]));
            if path.file_name().is_some_and(|n| n.to_string_lossy().contains(needle)) {
                found = Some((entry.modBaseAddr as u64, path));
                break;
            }
            ok = unsafe { Module32NextW(snap, &mut entry) };
        }
        unsafe { CloseHandle(snap) };
        found
    }
}

impl Drop for Proc {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.handle) };
    }
}
