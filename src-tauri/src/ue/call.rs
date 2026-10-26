//! Calls Unreal script functions inside the game: writes a tiny x64 stub that runs
//! `UObject::ProcessEvent(object, function, params)` and starts it on a new thread.
//! Used sparingly (calls run off the game thread), for things plain memory writes
//! can't do, like adding items through the game's own inventory code.

use super::objects::Ue;
use windows_sys::Win32::{
    Foundation::CloseHandle,
    System::{
        Memory::{VirtualAllocEx, VirtualFreeEx, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_EXECUTE_READWRITE},
        Threading::{CreateRemoteThread, WaitForSingleObject},
    },
};

/// A script function and where its parameters go.
pub struct Func {
    pub addr: u64,
    /// Parameter name -> byte offset in the parameter block.
    pub params: Vec<(String, u64)>,
    pub size: u64,
}

impl Ue {
    /// Finds `class_name::func_name` (a UFunction) and its parameter layout.
    pub fn func(&self, class_name: &str, func_name: &str) -> Option<Func> {
        let addr = self.all_objects().into_iter().find(|&o| {
            self.name_of(o).as_deref() == Some(func_name)
                && self.class_of(o).and_then(|c| self.name_of(c)).as_deref() == Some("Function")
                && self.outer_of(o).and_then(|x| self.name_of(x)).as_deref() == Some(class_name)
        })?;
        let params: Vec<(String, u64)> = self.props(addr).into_iter().map(|p| (p.name, p.offset as u64)).collect();
        // ParmsSize isn't reflected as a property; the largest offset plus generous slack covers it.
        let size = params.iter().map(|p| p.1).max().unwrap_or(0) + 0x40;
        Some(Func { addr, params, size })
    }

    /// Runs `obj->ProcessEvent(func, params)` in the game and returns the parameter block afterwards
    /// (return values are written back into it). Waits up to 2 seconds.
    pub fn call(&self, process_event: u64, obj: u64, func: &Func, params: &[u8]) -> Result<Vec<u8>, String> {
        let h = self.p.raw_handle();
        let size = 0x100 + func.size.max(params.len() as u64) as usize;
        let mem = unsafe { VirtualAllocEx(h, std::ptr::null(), size, MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READWRITE) } as u64;
        if mem == 0 {
            return Err("couldn't allocate memory in the game".into());
        }
        let block = mem + 0x100;
        let mut code: Vec<u8> = Vec::new();
        code.extend([0x48, 0x83, 0xEC, 0x28]); // sub rsp, 0x28
        code.extend([0x48, 0xB9]); // mov rcx, obj
        code.extend(obj.to_le_bytes());
        code.extend([0x48, 0xBA]); // mov rdx, func
        code.extend(func.addr.to_le_bytes());
        code.extend([0x49, 0xB8]); // mov r8, params
        code.extend(block.to_le_bytes());
        code.extend([0x48, 0xB8]); // mov rax, ProcessEvent
        code.extend(process_event.to_le_bytes());
        code.extend([0xFF, 0xD0]); // call rax
        code.extend([0x48, 0x83, 0xC4, 0x28]); // add rsp, 0x28
        code.extend([0x31, 0xC0, 0xC3]); // xor eax, eax; ret
        let mut padded = params.to_vec();
        padded.resize(func.size.max(params.len() as u64) as usize, 0);
        let ok = self.p.write_bytes(mem, &code) && self.p.write_bytes(block, &padded);
        let result = if !ok {
            Err("couldn't write the call stub".to_string())
        } else {
            let thread = unsafe { CreateRemoteThread(h, std::ptr::null(), 0, Some(std::mem::transmute(mem as usize)), std::ptr::null(), 0, std::ptr::null_mut()) };
            if thread.is_null() {
                Err(format!("couldn't start the call ({})", std::io::Error::last_os_error()))
            } else {
                let waited = unsafe { WaitForSingleObject(thread, 2000) };
                unsafe { CloseHandle(thread) };
                if waited != 0 {
                    // Still running: leave the memory allocated rather than free code that's executing.
                    return Err("the game didn't finish the call in time".into());
                }
                let mut out = vec![0u8; padded.len()];
                self.p.read_into(block, &mut out);
                Ok(out)
            }
        };
        unsafe { VirtualFreeEx(h, mem as _, 0, MEM_RELEASE) };
        result
    }
}
