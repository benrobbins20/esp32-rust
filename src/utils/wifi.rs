use anyhow::{Ok, Result};
use embedded_svc::channel;
use esp_idf_hal::{delay::FreeRtos, peripheral};
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    wifi::{
        AccessPointInfo, AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi,
    },
};

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
        sysloop: EspSystemEventLoop,
    ) -> anyhow::Result<Self> {
        // ASYNC wifi instance
        let mut wifi = EspWifi::new(modem, sysloop.clone(), None).unwrap();

        // set default configs for wifi client with station mode
        wifi.set_configuration(&Configuration::Client(ClientConfiguration::default()))?;

        // load and set the instance parameters
        let config = ClientConfiguration {
            ssid: ssid.try_into().unwrap(),
            password: password.try_into().unwrap(),
            auth_method: AuthMethod::WPA2Personal,
            ..Default::default()
        };
        wifi.set_configuration(&Configuration::Client(config.clone()))?;

        Ok(Self {
            wifi: Box::new(wifi), // boxed wifi instance essentially keeps the resource alive in static context
            ssid: ssid.to_string(),
            pass: password.to_string(),
            channel: None,
            aps: Vec::new(),
            sysloop,
            config: Some(config),
        })
    }

    pub fn connect(&mut self) -> Result<()> {
        // wrap the async wifi in blocking wifi
        let mut blocking_wifi = BlockingWifi::wrap(&mut *self.wifi, self.sysloop.clone()).unwrap();
        blocking_wifi.start().unwrap();
        
        // begin loop that scans for the access point and sets the channel
        loop {
            // read access points into a vector owned by WifiManager
            self.aps = blocking_wifi.scan()?;

            // ap returned as Ok(ap) if found
            if let Some(ap) = self
                .aps
                .iter()
                .find(|found_ap| found_ap.ssid.to_string() == self.ssid) {

                    // found ap, perform channel configuration
                    log::info!("found ap {:?}, channel {}", self.ssid, ap.channel);

                    // load channel into config and break out if ap is found
                    self.config.as_mut().unwrap().channel = Some(ap.channel);
                    blocking_wifi.set_configuration(&Configuration::Client(
                        self.config.as_ref().unwrap().clone(),
                    ))?;
                    log::info!("wifi config {:?}", self.config.as_ref().unwrap());
                    break;
                }

            // delay
            log::error!("could not find ap {}, rescanning...", self.ssid);
            FreeRtos::delay_ms(2000);
        }
        
        // connect to access point using blocking wifi, which is released after Ok()
        log::info!("connecting to {}", self.ssid);
        blocking_wifi.connect()?;
        blocking_wifi.wait_netif_up()?;
        let ip_info = self.wifi.sta_netif().get_ip_info()?;
        log::info!("DHCP info {:?}", ip_info);
        Ok(())
    }
}
