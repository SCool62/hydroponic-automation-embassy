use core::str::from_utf8;

use cyw43::Control;
use cyw43_pio::PioSpi;
use embassy_executor::Spawner;
use embassy_net::{Runner, Stack, tcp::TcpSocket};
use embassy_rp::{
    gpio::Output,
    peripherals::{DMA_CH0, PIO0},
};
use embassy_time::Timer;
use embedded_io_async::Write;
use heapless::{String, Vec};
use log::*;

use core::fmt::Write as _;

use crate::{WIFI_PWD, WIFI_SSID, state::{EcState, MACHINE_STATE, PhState, WaterLevelState}};

type Cyw43Runner = cyw43::Runner<'static, Output<'static>, PioSpi<'static, PIO0, 0, DMA_CH0>>;

#[embassy_executor::task]
pub async fn begin_hosting_task(
    spawner: Spawner,
    net_runner: Runner<'static, cyw43::NetDriver<'static>>,
    mut control: Control<'static>,
    stack: Stack<'static>,
) {
    // Begin network task
    spawner.spawn(net_task(net_runner)).unwrap();

    // Connect to wifi
    loop {
        if let Some(pwd) = WIFI_PWD {
            match control.join_wpa2(WIFI_SSID, pwd).await {
                Ok(_) => break,
                Err(err) => {
                    error!("Error joining network with status: {}", err.status);
                }
            }
        } else {
            match control.join_open(WIFI_SSID).await {
                Ok(_) => break,
                Err(err) => {
                    error!("Error joining network with status: {}", err.status);
                }
            }
        }
    }

    info!("waiting for DHCP...");
    while !stack.is_config_up() {
        Timer::after_millis(100).await;
    }
    info!("DHCP is now up!");

    let mut rx_buffer = [0; 4096];
    let mut tx_buffer = [0; 4096];
    let mut buf = [0; 4096];

    control.gpio_set(0, false).await;

    loop {
        let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
        //socket.set_timeout(Some(Duration::from_secs(10)));

        // info!("listening on TCP:1234");
        // if let Err(e) = socket.accept(1234).await {
        //     warn!("acception error: {:?}", e);
        //     continue;
        // }
        let _ = socket.accept(1234).await;

        info!("recieved connection from {:?}", socket.remote_endpoint());
        control.gpio_set(0, true).await;

        loop {
            let n = match socket.read(&mut buf).await {
                Ok(0) => {
                    warn!("read EOF");
                    break;
                }
                Ok(n) => n,
                Err(e) => {
                    warn!("read error: {:?}", e);
                    break;
                }
            };

            info!("rxd {}", from_utf8(&buf[..n]).unwrap());

            let response = handle_request(&buf[..n]).await;
            match socket.write_all(&response).await {
                Ok(()) => {
                    break;
                }
                Err(e) => {
                    warn!("write error: {:?}", e);
                    break;
                }
            };
        }
        control.gpio_set(0, false).await;
    }
}

#[embassy_executor::task]
pub async fn cyw43_task(runner: Cyw43Runner) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, cyw43::NetDriver<'static>>) -> ! {
    runner.run().await;
}

// Accepts the request from the client and returns the appropriate response
async fn handle_request(req: &[u8]) -> Vec<u8, 64> {
    let mut sections = req.split(|b| b == &b' ');
    let (method, path) = (
        from_utf8(sections.next().unwrap()).unwrap(),
        from_utf8(sections.next().unwrap()).unwrap(),
    );

    // Possible paths:
    // / => Hello World
    // /ph => (high/good/low), (ph value)
    // /ec => (high, good, low), (ec value)
    // /waterlevel => (good/low)
    // NOT IMPLEMENTED!!!
    // /all => (high/good/low), (ph value), (high/good/low), (ec value), (good/low)
    let good_status_line = "HTTP/1.1 200 OK\r\n";
    match method {
        "GET" => {
            match path {
                "/" => {
                    Vec::from_slice(b"HTTP/1.1 200 OK\r\nContent-Length: 13\r\n\r\nHello, world!")
                        .unwrap()
                }
                // FIXME: "Connection Reset"
                "/ph" => {
                    info!("Hit ph path");
                    let mut resp: String<64> = String::new();
                    resp.push_str(good_status_line).expect("BUFFER TOO SMALL!");

                    let mut content: String<16> = String::new();
                    // TODO: Get state
                    let state = MACHINE_STATE.lock().await.as_ref().unwrap().ph;
                    match state {
                        PhState::Good(v) => {
                            core::write!(&mut content, "good, {:.2}", v).expect("BUFFER TOO SMALL");
                        }
                        PhState::High(v) => {
                            core::write!(&mut content, "high, {:.2}", v)
                                .expect("BUFFER TOO SMALL!");
                        }
                        PhState::Low(v) => {
                            core::write!(&mut content, "low, {:.2}", v).expect("BUFFER TOO SMALL!");
                        }
                        PhState::Unknown => {
                            content.push_str("unk").expect("BUFFER TOO SMALL");
                        }
                    }
                    core::write!(
                        &mut resp,
                        "Content-Length: {}\r\n\r\n{}",
                        content.len(),
                        content
                    )
                    .expect("BUFFER TOO SMALL!");
                    Vec::from_slice(resp.as_bytes()).expect("BUFFER TOO SMALL")
                    //Vec::from_slice(b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\npH endpoint").unwrap()
                }
                "/ec" => {
                    info!("Hit ec path");
                    let mut resp: String<47> = String::new();
                    resp.push_str(good_status_line).expect("BUFFER TOO SMALL!");

                    let mut content: String<10> = String::new();
                    // TODO: Get state
                    let state = MACHINE_STATE.lock().await.as_ref().unwrap().ec;
                    match state {
                        EcState::Good(v) => {
                            core::write!(&mut content, "good, {:.2}", v)
                                .expect("BUFFER TOO SMALL!");
                        }
                        EcState::High(v) => {
                            core::write!(&mut content, "high, {:.2}", v)
                                .expect("BUFFER TOO SMALL!");
                        }
                        EcState::Low(v) => {
                            core::write!(&mut content, "low, {:.2}", v).expect("BUFFER TOO SMALL!");
                        }
                        EcState::Unknown => content.push_str("unk").expect("BUFFER TOO SMALL"),
                    }
                    core::write!(
                        &mut resp,
                        "Content-Length: {}\r\n\r\n{}",
                        content.len(),
                        content
                    )
                    .expect("BUFFER TOO SMALL!");
                    Vec::from_slice(resp.as_bytes()).expect("BUFFER TOO SMALL")
                    //Vec::from_slice(b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\n\r\nnec endpoint").unwrap()
                }
                "/waterlevel" => {
                    // TODO: Get state
                    let state = MACHINE_STATE.lock().await.as_ref().unwrap().water_level;
                    match state {
                        WaterLevelState::Good => {
                            Vec::from_slice(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\ngood")
                                .unwrap()
                        }
                        WaterLevelState::Low => {
                            Vec::from_slice(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nlow").unwrap()
                        }
                        WaterLevelState::Unknown => {
                            Vec::from_slice(b"HTTP/1.1 200 OK\r\nContent-Length: 7\r\n\r\nunknown")
                                .unwrap()
                        }
                    }
                }
                _ => Vec::from_slice(b"HTTP/1.1 404 NOT FOUND\r\n").unwrap(),
            }
        }
        "HEAD" => Vec::from_slice(b"HTTP/1.1 200 OK\r\n").unwrap(),
        _ => Vec::from_slice(b"HTTP/1.1 501 Not Implemented\r\n").unwrap(),
    }
}
