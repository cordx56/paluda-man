use chrono::{Timelike, Utc};
use embedded_svc::ota::*;
use esp_idf_svc::{
    hal::{gpio::*, reset::restart},
    http::{
        server::{self, EspHttpServer},
        Method,
    },
    io::{Read, Write},
    nvs::*,
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

const SCHEDULE_ON_TAG: &str = "schedule_on";
const SCHEDULE_OFF_TAG: &str = "schedule_off";

#[derive(serde::Deserialize)]
struct Query {
    toggle: Option<i8>,
    schedule_on: Option<i8>,
    schedule_off: Option<i8>,
}

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let _wifi = wifi();

    let nvs_default_partition = EspDefaultNvsPartition::take()?;
    let nvs = EspNvs::new(nvs_default_partition, "schedule", true)?;

    let mut server = EspHttpServer::new(&server::Configuration {
        stack_size: 10240,
        ..Default::default()
    })?;
    server.fn_handler("/", Method::Get, |req| {
        let on = nvs.get_i8(SCHEDULE_ON_TAG)?.unwrap_or(-1);
        let off = nvs.get_i8(SCHEDULE_OFF_TAG)?.unwrap_or(-1);
        req.into_ok_response()?
            .write_all(html(on, off).as_bytes())
            .map(|_| ())
    })?;
    server.fn_handler("/", Method::Post, |mut req| {
        let mut buf = [0; 1024];
        let length = req.read(&mut buf)?;
        let query: Query = if let Ok(query) = serde_qs::from_bytes(&buf[0..length]) {
            query
        } else {
            return req
                .into_status_response(400)?
                .write_all("request parse error".as_bytes())
                .map(|_| ());
        };
        if query.toggle.unwrap_or(0) == 1 {
            unsafe {
                if LIGHT == 0 {
                    LIGHT = 1;
                } else {
                    LIGHT = 0;
                }
            }
        }
        if let Some(on) = query.schedule_on {
            nvs.set_i8(SCHEDULE_ON_TAG, on)?;
        }
        if let Some(off) = query.schedule_off {
            nvs.set_i8(SCHEDULE_OFF_TAG, off)?;
        }
        let on = nvs.get_i8(SCHEDULE_ON_TAG)?.unwrap_or(-1);
        let off = nvs.get_i8(SCHEDULE_OFF_TAG)?.unwrap_or(-1);
        req.into_ok_response()?
            .write_all(html(on, off).as_bytes())
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

    let _sntp = EspSntp::new(&SntpConf {
        servers: NTP_SERVER,
        operating_mode: OperatingMode::Poll,
        sync_mode: SyncMode::Immediate,
    })?;
    info!("SNTP init");

    let mut out = PinDriver::output(unsafe { Gpio2::new() })?;
    loop {
        //info!("Current time: {:?}", std::time::SystemTime::now());
        let now = Utc::now();
        let schedule_on = nvs.get_i8(SCHEDULE_ON_TAG).unwrap_or(None).unwrap_or(-1);
        let schedule_off = nvs.get_i8(SCHEDULE_OFF_TAG).unwrap_or(None).unwrap_or(-1);
        if now.hour() == schedule_on as u32 && now.minute() == 0 && now.second() == 0 {
            unsafe {
                LIGHT = 1;
            }
        }
        if now.hour() == schedule_off as u32 && now.minute() == 0 && now.second() == 0 {
            unsafe {
                LIGHT = 0;
            }
        }
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

fn html(on: i8, off: i8) -> String {
    let light = unsafe { LIGHT };
    let light = if light == 0 { "OFF" } else { "ON" };
    let now = Utc::now();
    let time = format!("{}:{}:{}", now.hour(), now.minute(), now.second());
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
        <button type="submit" name="toggle" value="1">Toggle</button>
    </form>
    <p>
        UTC: {}
    </p>
    <form method="post">
        schedule_on: {}, <input type="number" name="schedule_on" />
        <button type="submit">Update</button>
    </form>
    <form method="post">
        schedule_off: {}, <input type="number" name="schedule_off" />
        <button type="submit">Update</button>
    </form>
  </body>
</html>
"#,
        light, time, on, off
    )
}
