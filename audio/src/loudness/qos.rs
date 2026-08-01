//! 把当前线程降为后台优先级。
//!
//! 响度分析要与播放共存，而**真正知道此刻该给后台任务多少资源的是操作系统调度器**
//! ——它知道前台在跑什么、电池还剩多少、机器烫不烫。QoS 就是把这个判断交给它的正规
//! 途径，比我们在用户机器上跑一次 benchmark 再自己拍板要准得多（那还得先打扰用户
//! 一次才能得出「为了不打扰用户该用几个线程」）。
//!
//! 三平台各有各的说法，语义都是「这活儿不急，别抢前台的」：
//!
//! | 平台 | 手段 | 验证程度 |
//! | --- | --- | --- |
//! | macOS | `pthread_set_qos_class_self_np(QOS_CLASS_BACKGROUND)` | 运行时读回校验（见测试） |
//! | Windows | `SetThreadPriority(THREAD_MODE_BACKGROUND_BEGIN)` | 仅 FFI 签名交叉编译检查 |
//! | Linux | `nice(+10)` 与 `ioprio_set(IDLE)` | 仅 FFI 签名交叉编译检查 |
//!
//! 后两者尚未在目标平台上实机验证——`cargo check --target x86_64-pc-windows-msvc`
//! 目前也过不去，卡在 cpal 0.18.1 与 windows-core 0.62 不兼容（与本模块无关），
//! 所以这两段是抄进一个只依赖 libc / windows-sys 的最小 crate 里单独检查的。
//! **失败也只是降级为普通优先级，不影响正确性**，因此一律忽略返回值，
//! 不把它变成一条能让分析停摆的错误路径。

/// 尽力把当前线程降为后台优先级。返回是否确实调用成功。
///
/// 失败不是错误：拿不到低优先级，最坏结果是分析与前台抢一点 CPU，而不是分析不对。
pub fn apply_background() -> bool {
    imp::apply_background()
}

#[cfg(target_os = "macos")]
mod imp {
    pub fn apply_background() -> bool {
        // 安全性：只改本线程自身的 QoS，不涉及任何指针或跨线程状态。
        let rc = unsafe {
            libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_BACKGROUND, 0)
        };
        rc == 0
    }

    /// 读回当前线程的 QoS class（测试用：设了什么必须能验出来）。
    #[cfg(test)]
    pub fn current_class() -> u32 {
        let mut class = libc::qos_class_t::QOS_CLASS_UNSPECIFIED;
        // 安全性：`class` 是栈上的合法可写对象，priority 传空指针是 API 允许的。
        unsafe {
            libc::pthread_get_qos_class_np(libc::pthread_self(), &mut class, std::ptr::null_mut());
        }
        class as u32
    }
}

#[cfg(target_os = "windows")]
mod imp {
    use windows_sys::Win32::System::Threading::{
        GetCurrentThread, SetThreadPriority, THREAD_MODE_BACKGROUND_BEGIN,
    };

    pub fn apply_background() -> bool {
        // THREAD_MODE_BACKGROUND_BEGIN 同时降 CPU 与 I/O 优先级，正是这里要的语义；
        // 它只对**自身线程**合法，因此必须传 GetCurrentThread 的伪句柄。
        // 安全性：伪句柄无需释放，调用不涉及任何我们持有的指针。
        unsafe { SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN) != 0 }
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod imp {
    pub fn apply_background() -> bool {
        // Linux 的 nice 是**每线程**的，所以在 worker 线程内调用只影响它自己。
        // 安全性：两个调用都不接受指针，失败只返回错误码。
        let niced = unsafe { libc::nice(10) } != -1;
        // 光降 CPU 不够：分析要把整个文件读一遍，磁盘才是它最容易挡住前台的地方。
        // IOPRIO_WHO_PROCESS = 1、who = 0 表示调用线程；IOPRIO_CLASS_IDLE = 3 位于高 13 位。
        const IOPRIO_WHO_PROCESS: libc::c_long = 1;
        const IOPRIO_CLASS_IDLE: libc::c_long = 3;
        let io = unsafe {
            libc::syscall(
                libc::SYS_ioprio_set,
                IOPRIO_WHO_PROCESS,
                0,
                IOPRIO_CLASS_IDLE << 13,
            )
        } == 0;
        niced && io
    }
}

#[cfg(not(any(unix, target_os = "windows")))]
mod imp {
    pub fn apply_background() -> bool {
        false
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    /// QoS 常量 0x09 = `QOS_CLASS_BACKGROUND`。
    const BACKGROUND: u32 = 0x09;

    #[test]
    fn background_qos_is_actually_applied_to_the_calling_thread() {
        // 在独立线程里验：设错了会连带影响后续测试，也说明不了「只改自己」。
        let handle = std::thread::spawn(|| {
            let before = imp::current_class();
            assert!(apply_background(), "macOS 上不该失败");
            (before, imp::current_class())
        });
        let (before, after) = handle.join().unwrap();
        assert_ne!(before, BACKGROUND, "默认不该已经是后台级");
        assert_eq!(after, BACKGROUND, "设完必须读得回来");
        assert_ne!(
            imp::current_class(),
            BACKGROUND,
            "只改调用线程，主线程不受影响"
        );
    }
}
