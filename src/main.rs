use embedded_svc::ota::*;
use esp_idf_svc::{
    hal::{gpio::*, reset::restart},
    http::{
        server::{self, EspHttpServer},
        Method,
    },
    io::Write,
    ota::*,
    sntp::*,
    sys::EspError,
    wifi::EspWifi,
};
use log::info;

const SSID: &str = env!("WIFI_SSID");
const PASS: &str = env!("WIFI_PASS");
const NTP_SERVER: [&str; 1] = ["ntp.nict.jp"];
static mut LIGHT: usize = 0;

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let _wifi = wifi();
    let _sntp = EspSntp::new(&SntpConf {
        servers: NTP_SERVER,
        operating_mode: OperatingMode::Poll,
        sync_mode: SyncMode::Immediate,
    })?;
    info!("SNTP init");

    let mut server = EspHttpServer::new(&server::Configuration {
        stack_size: 10240,
        ..Default::default()
    })?;
    server.fn_handler("/", Method::Get, |req| {
        req.into_ok_response()?
            .write_all(html(unsafe { LIGHT }).as_bytes())
            .map(|_| ())
    })?;
    server.fn_handler("/", Method::Post, |req| {
        unsafe {
            if LIGHT == 0 {
                LIGHT = 1;
            } else {
                LIGHT = 0;
            }
        }
        req.into_ok_response()?
            .write_all(html(unsafe { LIGHT }).as_bytes())
            .map(|_| ())
    })?;
    server.fn_handler("/ota", Method::Post, |mut req| {
        let mut ota = EspOta::new()?;
        let update = ota.initiate_update()?;
        info!("OTA initiate");
        info!("Start OTA update...");
        let updated = update.update(&mut req, |_, _| ());
        restart();
        req.into_ok_response()?
            .write_all(format!("{:?}", updated).as_bytes())
            .map(|_| ())
    })?;

    let mut out = PinDriver::output(unsafe { Gpio2::new() })?;
    loop {
        //info!("Current time: {:?}", std::time::SystemTime::now());
        if unsafe { LIGHT } == 0 {
            out.set_low();
        } else {
            out.set_high();
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

fn wifi() -> Result<EspWifi<'static>, EspError> {
    use esp_idf_svc::eventloop::*;
    use esp_idf_svc::hal::peripherals::Peripherals;
    use esp_idf_svc::wifi::*;
    let peripherals = Peripherals::take()?;
    let sys_loop = EspSystemEventLoop::take()?;
    let mut esp_wifi = EspWifi::new(peripherals.modem, sys_loop.clone(), None)?;
    let mut wifi = BlockingWifi::wrap(&mut esp_wifi, sys_loop.clone())?;
    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: SSID.try_into().unwrap(),
        password: PASS.try_into().unwrap(),
        ..Default::default()
    }))?;

    wifi.start()?;
    info!("WiFi started");

    wifi.connect()?;
    info!("WiFi connected");

    wifi.wait_netif_up()?;
    info!("WiFi netif up");

    Ok(esp_wifi)
}

fn html(light: usize) -> String {
    let light = if light == 0 { "OFF" } else { "ON" };
    format!(
        r#"<!doctype html>
<html>
  <head>
    <meta charset="utf-8">
    <title>paluda-man</title>
  </head>
  <body>
    <p>current: {}</p>
    <form method="post">
        <button type="submit">Toggle</button>
    </form>
  </body>
</html>
"#,
        light
    )
}
