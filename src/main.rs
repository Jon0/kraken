extern crate libusb;

use std::time::Duration;


fn print_endpoint(endpoint: libusb::EndpointDescriptor) {
    println!("Endpoint address {:02x}", endpoint.address());
    println!("Endpoint number {:02x}", endpoint.number());
    println!("Endpoint direction {:?}", endpoint.direction());
    println!("Endpoint transfer {:?}", endpoint.transfer_type());
    println!("Endpoint sync {:?}", endpoint.sync_type());
    println!("Endpoint usage {:?}", endpoint.usage_type());
    println!("Endpoint packet size {}", endpoint.max_packet_size());
}


fn print_device(device: libusb::Device) {
    let device_desc = device.device_descriptor().unwrap();
    println!("Bus {:03} Device {:03} ID {:04x}:{:04x}",
        device.bus_number(),
        device.address(),
        device_desc.vendor_id(),
        device_desc.product_id());

    let config = device.active_config_descriptor().unwrap();
    println!("Number {}, Interfaces {}", config.number(), config.num_interfaces());

    for interface in config.interfaces() {
        for descriptor in interface.descriptors() {
            println!("Endpoints {}", descriptor.num_endpoints());
            for endpoint in descriptor.endpoint_descriptors() {
                print_endpoint(endpoint);
            }
        }
    }
}


fn open<'a>(mut handle: libusb::DeviceHandle<'a>) {
    //let handle = device.open().unwrap();
    let interface_num = 0x1;
    handle.claim_interface(interface_num);
    let mut buf: [u8; 64] = [0; 64];
    let result = handle.read_interrupt(0x81, &mut buf, Duration::from_secs(5)).unwrap();
    handle.release_interface(interface_num);

    // print state
    println!("Result {}", result);
    for i in 0..result {
        println!("Data[{}]: {:02x} ({})", i, buf[i], buf[i]);
    }

    let temp: f32 = (buf[1] as f32) + (buf[2] as f32 / 10.0);
    let fan: u16 = buf[4] as u16 | ((buf[3] as u16) << 8);
    let pump: u16 = buf[6] as u16 | ((buf[5] as u16) << 8);
    println!("Temp: {} C, Fan: {} RPM, Pump: {} RPM", temp, fan, pump);
}


fn main() {
    let mut context = libusb::Context::new().unwrap();
    let handle = context.open_device_with_vid_pid(0x1e71, 0x170e).unwrap();

    // print all device information
    for mut device in context.devices().unwrap().iter() {
        let device_desc = device.device_descriptor().unwrap();
        if device_desc.vendor_id() == 0x1e71 {
            print_device(device);
        }
    }

    // read status
    open(handle);
}
