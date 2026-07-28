//! 列出输出设备及其支持的配置：`cargo run -p shannon-audio --example devices`
//!
//! 开发期诊断工具。协商失败时第一时间要看的就是这份清单——
//! 「设备不支持 44100 Hz」究竟是真不支持，还是我们的挑选逻辑有问题。

use cpal::traits::{DeviceTrait, HostTrait};

fn main() {
    let host = cpal::default_host();
    let default = host.default_output_device().and_then(|d| d.description().ok());
    let default_name = default.as_ref().map(|d| d.name().to_string());

    for device in host.output_devices().expect("枚举设备失败") {
        let name = device.description().map(|d| d.name().to_string()).unwrap_or_default();
        let mark = if Some(&name) == default_name.as_ref() { " ←默认" } else { "" };
        println!("{name}{mark}");
        match device.supported_output_configs() {
            Ok(configs) => {
                for c in configs {
                    println!(
                        "    {} ch · {} – {} Hz · {}",
                        c.channels(),
                        c.min_sample_rate(),
                        c.max_sample_rate(),
                        c.sample_format()
                    );
                }
            }
            Err(e) => println!("    读取能力失败：{e}"),
        }
    }
}
