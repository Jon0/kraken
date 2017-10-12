extern crate rand;
extern crate libusb;

use std::str;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::fs::File;
use std::time::Duration;
use std::u8;
use rand::Rng;

type Temprature = f32;


trait TempratureSensor {
    fn sensor_name(&self) -> &str;
    fn sensor_read(&mut self) -> Temprature;
}


trait PwmController {
    fn control_name(&self) -> &str;
    fn control_get_rpm(&self) -> u32;
    fn control_set_power(&mut self, u8);
}


/*
 * Reads sensor state from hwmon files
 */
struct SysfsSensor {
    name: String,
    file: File,
}


impl SysfsSensor {
    fn open(sensor_name: &str, file_path: &str) -> SysfsSensor {
        return SysfsSensor {
            name: sensor_name.to_string(),
            file: File::open(file_path).unwrap(),
        }
    }
}


impl TempratureSensor for SysfsSensor {
    fn sensor_name(&self) -> &str {
        return self.name.as_str();
    }

    fn sensor_read(&mut self) -> Temprature {
        let mut buf: [u8; 32] = [0; 32];

        // read from start of file
        self.file.seek(SeekFrom::Start(0));
        let result = self.file.read(&mut buf).unwrap();
        let temp_str = str::from_utf8(&buf[0..result - 1]).unwrap();
        let temp_f32 = (temp_str.parse::<i32>().unwrap() as f32) / 1000.0;
        return temp_f32;
    }
}


/*
 * current state read from device
 */
struct Status {
    temp: f32,
    fan: u16,
    pump: u16,
}


impl Status {
    fn decode_status(buf: &[u8], verbose: bool) -> Status {
        // bytes 7 to 16 are usually the same
        let expected: [u8; 10] = [0x00, 0x00, 0x00, 0xff, 0x02, 0x00, 0x01, 0x08, 0x1e, 0x00];
        if verbose {
            if buf.len() != 17 {
                println!("Unexpected status length: {}", buf.len());
            }
            if buf[0] != 0x04 {
                println!("Unexpected first byte: {}", buf[0]);
            }
            for i in 7..buf.len() {
                let expected_byte = expected[i - 7];
                if buf[i] != expected_byte {
                    println!("Unexpected byte[{}]: {:02x}, expected {:02x}", i, buf[i], expected_byte);
                }
            }
        }
        if buf.len() > 6 {
            return Status {
                temp: (buf[1] as f32) + (buf[2] as f32 / 9.0),
                fan: buf[4] as u16 | ((buf[3] as u16) << 8),
                pump: buf[6] as u16 | ((buf[5] as u16) << 8),
            }
        }
        else {
            return Status {
                temp: 0.0,
                fan: 0,
                pump: 0,
            }
        }
    }
}


/*
 * a single color
 */
#[derive(Debug, Copy, Clone)]
struct RGB {
    r: u8,
    g: u8,
    b: u8,
}


impl RGB {
    fn rand() -> RGB {
        RGB {r: rand::random(), g: rand::random(), b: rand::random()}
    }
}


fn color_msg(mode: u8, seq: u8, text: RGB, colors: &[RGB; 8]) -> [u8; 32] {
    let mut result: [u8; 32] = [0; 32];
    result[0] = 0x02;
    result[1] = 0x4c;
    result[2] = 0x00;
    result[3] = mode;
    result[4] = 0x02 | ((seq & 0x07) << 5);
    result[5] = text.g;
    result[6] = text.r;
    result[7] = text.b;
    for i in 0..8 {
        result[i*3 + 8] = text.r;
        result[i*3 + 9] = text.g;
        result[i*3 + 10] = text.b;
    }
    return result;
}


/*
 * sets and reads fan and pump speeds
 */
struct UsbController<'a> {
    name: String,
    handle: libusb::DeviceHandle<'a>,
    interface: u8,
}


impl<'a> UsbController<'a> {
    fn open(sensor_name: &str, device: &'a libusb::Device) -> UsbController<'a> {
        return UsbController {
            name: sensor_name.to_string(),
            handle: device.open().unwrap(),
            interface: 0x00,
        }
    }

    fn claim(&mut self) {
        self.handle.detach_kernel_driver(self.interface);
        self.handle.claim_interface(self.interface);
    }

    fn release(&mut self) {
        self.handle.release_interface(self.interface);
    }

    fn sensor_name(&self) -> &str {
        return self.name.as_str();
    }

    fn get_status(&mut self) -> Status {
        let mut buf: [u8; 64] = [0; 64];
        let result = self.handle.read_interrupt(0x81, &mut buf, Duration::from_secs(1)).unwrap();
        return Status::decode_status(&buf[0..result], true);
    }

    fn set_fan(&mut self, fan_speed: u8) {
        let mut buf: [u8; 5] = [0x02, 0x4d, 0x00, 0x00, fan_speed];
        if fan_speed > 100 {
            buf[4] = 100;
        }
        let result = self.handle.write_interrupt(0x01, &buf, Duration::from_secs(1)).unwrap();
    }

    fn set_pump(&mut self, pump_speed: u8) {
        let mut buf: [u8; 5] = [0x02, 0x4d, 0x40, 0x00, pump_speed];
        if pump_speed > 100 {
            buf[4] = 100;
        }
        let result = self.handle.write_interrupt(0x01, &buf, Duration::from_secs(1)).unwrap();
    }

    fn set_color(&mut self, text: RGB, colors: &[RGB; 8]) {
        let mode = 0x06;
        let buf = color_msg(mode, 0, text, &colors);
        let result = self.handle.write_interrupt(0x01, &buf, Duration::from_secs(1)).unwrap();
    }

    fn set_color_random(&mut self) {
        let mode = 0x04;
        for seq in 0..8 {
            let text = RGB::rand();
            let colors = [RGB::rand(); 8];
            let buf = color_msg(mode, seq, text, &colors);
            let result = self.handle.write_interrupt(0x01, &buf, Duration::from_secs(1)).unwrap();
        }
    }
}


fn print_endpoint(endpoint: libusb::EndpointDescriptor) {
    println!("Endpoint address {:02x}", endpoint.address());
    println!("Endpoint number {:02x}", endpoint.number());
    println!("Endpoint direction {:?}", endpoint.direction());
    println!("Endpoint transfer {:?}", endpoint.transfer_type());
    println!("Endpoint sync {:?}", endpoint.sync_type());
    println!("Endpoint usage {:?}", endpoint.usage_type());
    println!("Endpoint packet size {}", endpoint.max_packet_size());
}


fn print_device(device: &libusb::Device) {
    let device_desc = device.device_descriptor().unwrap();
    println!("Bus {:03} Device {:03} ID {:04x}:{:04x}",
        device.bus_number(),
        device.address(),
        device_desc.vendor_id(),
        device_desc.product_id());

    let config = device.active_config_descriptor().unwrap();
    println!("Number {}, Interfaces {}", config.number(), config.num_interfaces());

    for interface in config.interfaces() {
        println!("Interface {:04x}", interface.number());
        for descriptor in interface.descriptors() {
            println!("Endpoints {}", descriptor.num_endpoints());
            for endpoint in descriptor.endpoint_descriptors() {
                print_endpoint(endpoint);
            }
        }
    }
}


struct SensorReading {
    name: String,
    value: f32,
}


/*
 * monitors sensors and adjusts fan speed
 */
struct Monitor<'a> {
    sensor_file: Vec<SysfsSensor>,
    sensor_usb: Vec<UsbController<'a>>,
}


impl<'a> Monitor<'a> {
    fn new() -> Monitor<'a> {
        return Monitor {
            sensor_file: Vec::new(),
            sensor_usb: Vec::new(),
        }
    }

    fn add_file_monitor(&mut self, sensor_name: &str, filepath: &str) {
        self.sensor_file.push(SysfsSensor::open(sensor_name, filepath));
    }

    fn add_usb_monitor(&mut self, sensor_name: &str, device: &'a libusb::Device) {
        self.sensor_usb.push(UsbController::open(sensor_name, &device));
    }

    fn read_tempratures(&mut self) -> Vec<SensorReading> {
        let mut result = Vec::new();
        for file_device in self.sensor_file.iter_mut() {
            result.push(SensorReading{name: file_device.sensor_name().to_string(), value: file_device.sensor_read()});
        }
        for usb_device in self.sensor_usb.iter_mut() {
            let status = usb_device.get_status();
            result.push(SensorReading{name: usb_device.sensor_name().to_string(), value: status.temp});
        }
        return result;
    }

    fn run(&mut self) {
        for usb_device in self.sensor_usb.iter_mut() {
            usb_device.claim();
            usb_device.set_color_random();
        }


        let mut previous_temp = 0.0;
        let mut previous_speed = 0;
        loop {
            let temps = self.read_tempratures();
            let mut highest_temp = 0.0;
            for temp in temps.iter() {
                if temp.value > highest_temp {
                    highest_temp = temp.value;
                }
            }

            // printout when values change
            if highest_temp != previous_temp {
                previous_temp = highest_temp;
                for temp in temps {
                    println!("{} Temp {:.2} C", temp.name, temp.value);
                }

                // modify fan speed
                let target_speed = (100.0 * highest_temp / 70.0) as u8;

                // smooth over large changes
                let adjusted_speed: u32 = ((previous_speed as u32 * 7) + target_speed as u32) / 8;
                let new_speed = adjusted_speed as u8;
                previous_speed = new_speed;

                println!("Setting fan: {}, pump {}", new_speed, new_speed);
                for usb_device in self.sensor_usb.iter_mut() {
                    //usb_device.set_color(new_speed, 0, 0);
                    usb_device.set_fan(new_speed);
                    usb_device.set_pump(new_speed);
                }
            }
        }

        for usb_device in self.sensor_usb.iter_mut() {
            usb_device.release();
        }
    }
}


fn monitor_device(board_temp: &mut SysfsSensor, cpu_temp: &mut SysfsSensor, usb_device: &mut UsbController) {
    let mut current_temp = 0.0;
    loop {
        let status = usb_device.get_status();
        let board_reading = board_temp.sensor_read();
        let cpu_reading = cpu_temp.sensor_read();
        let monitor = (board_reading + cpu_reading + status.temp) as f32 / 3.0;
        if monitor != current_temp {
            current_temp = monitor;
            println!("Board Temp {:.2} C, CPU Temp {:.2} C, Water Temp: {:.2} C, Fan: {} RPM, Pump: {} RPM", board_reading, cpu_reading, status.temp, status.fan, status.pump);
        }
    }

}


fn select_device(device: libusb::Device) {

    // print all device information
    print_device(&device);

    // add devices to monitor
    let mut monitor = Monitor::new();
    //monitor.add_file_monitor("Board", "/sys/class/hwmon/hwmon4/temp2_input");
    monitor.add_file_monitor("CPU", "/sys/class/hwmon/hwmon0/temp1_input");
    monitor.add_usb_monitor("Water", &device);
    monitor.run();
}


fn main() {
    // usb id
    let vendor_id = 0x1e71;
    let product_id = 0x170e;
    let mut context = libusb::Context::new().unwrap();

    // device selection
    for mut device in context.devices().unwrap().iter() {
        let device_desc = device.device_descriptor().unwrap();
        if device_desc.vendor_id() == vendor_id && device_desc.product_id() == product_id {
            select_device(device);
        }
    }
}
