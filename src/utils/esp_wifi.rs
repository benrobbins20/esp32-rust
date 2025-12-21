use anyhow::{Ok, Result};
use embedded_svc::channel;
use esp_idf_hal::peripheral;
use esp_idf_svc::{eventloop::EspSystemEventLoop, wifi::{AccessPointInfo, AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi}};

// wifi manager with static reference
pub struct WifiManager {
    ssid: String,
    pass: String,
    wifi: Box<EspWifi<'static>>,
    channel: Option<u8>,
    aps: Vec<AccessPointInfo>,
    sysloop: EspSystemEventLoop,
    config: Option<ClientConfiguration>,
}

impl WifiManager {
    pub fn new(
        ssid: &str,
        password: &str,
        modem: impl peripheral::Peripheral<P = esp_idf_hal::modem::Modem> + 'static,
        sysloop: EspSystemEventLoop
    ) -> anyhow::Result<Self> {
        // ASYNC wifi instance
        let mut wifi = EspWifi::new(modem, sysloop.clone(), None).unwrap();
        wifi.set_configuration(&Configuration::Client(ClientConfiguration::default()))?;
        let config = ClientConfiguration {
            ssid: ssid.try_into().unwrap(),
            password: password.try_into().unwrap(),
            auth_method: AuthMethod::WPA2Personal,
            ..Default::default()
        };
        wifi.set_configuration(&Configuration::Client((config.clone())))?;
        Ok(Self {
            wifi: Box::new(wifi), // boxed wifi instance you'll use 
            ssid: ssid.to_string(), 
            pass: password.to_string(), 
            channel: None, 
            aps: Vec::new(),
            sysloop,
            config: Some(config),
        })
    }

    // pub fn configure(&mut self) -> Result<()> {
    //     let mut client_config = ClientConfiguration::default();
    //     client_config.ssid = self.ssid.clone();
    //     client_config.password = self.pass.clone();
    //     if let Some(channel) = self.channel {
    //         client_config.channel = Some(channel);
    //     }
    //     self.wifi.set_configuration(&Configuration::Client(client_config))?;
    //     Ok(())
    // }

    pub fn set_channel(&mut self) {
        // wrap the async wifi in blocking wifi
        let mut blocking_wifi = BlockingWifi::wrap(&mut *self.wifi, self.sysloop.clone()).unwrap();
        log::info!("scanning for ap's");
        blocking_wifi.start().unwrap();
        self.aps = blocking_wifi.scan().unwrap();
        let ap = self.aps
            .iter()
            .find(|found_ap| found_ap.ssid.to_string() == self.ssid);
        // 
        self.channel = if let Some(ap) = ap {
            log::info!("found ap {:?}, channel {}", self.ssid, ap.channel);
            Some(ap.channel)
        }
        // set channel to None
        else {
            log::error!("could not find ap {}", self.ssid);
            None
        };
        self.config.as_mut().unwrap().channel = self.channel;
        self.wifi.set_configuration(&Configuration::Client(self.config.as_ref().unwrap().clone())).unwrap();
        log::info!("setting wifi channel to {:?}", self.channel);
        // print the config
        log::info!("wifi config: {:?}", self.config);

    }

    pub fn connect(&mut self) -> Result<()> {
        let mut blocking_wifi = BlockingWifi::wrap(&mut *self.wifi, self.sysloop.clone()).unwrap();
        if !blocking_wifi.is_started()? {
            blocking_wifi.start()?;
        }
        log::info!("connecting to {}", self.ssid);
        blocking_wifi.connect()?;
        blocking_wifi.wait_netif_up()?;
        let ip_info = self.wifi.sta_netif().get_ip_info()?;
        log::info!("DHCP info {:?}", ip_info);
        Ok(())
    }
}