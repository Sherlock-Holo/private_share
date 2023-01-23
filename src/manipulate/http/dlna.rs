use std::future::ready;
use std::time::Duration;

use futures_util::{Stream, TryStreamExt};
use http::{StatusCode, Uri};
use rupnp::ssdp::{SearchTarget, URN};
use rupnp::Device;
use tap::TapFallible;
use tokio::net::UdpSocket;
use tracing::{debug, error, info, instrument};
use xml::escape::escape_str_attribute;

const AV_TRANSPORT: URN = URN::service("schemas-upnp-org", "AVTransport", 1);
const PAYLOAD_PLAY: &str = r#"
    <InstanceID>0</InstanceID>
    <Speed>1</Speed>
"#;
const SET_AV_TRANSPORT_URI_ACTION: &str = "SetAVTransportURI";
const PLAY_ACTION: &str = "Play";

#[derive(Debug, Clone)]
pub struct TV {
    device: Device,
}

impl TV {
    #[instrument(err)]
    pub async fn from_url(url: Uri) -> anyhow::Result<Option<Self>> {
        match Device::from_url(url).await {
            Err(rupnp::Error::HttpErrorCode(StatusCode::NOT_FOUND)) => {
                error!("tv not found");

                Ok(None)
            }

            Err(err) => {
                error!(%err, "create dlna device failed");

                Err(err.into())
            }

            Ok(device) => Ok(Some(Self { device })),
        }
    }

    pub fn friend_name(&self) -> &str {
        self.device.friendly_name()
    }

    pub fn url(&self) -> String {
        self.device.url().to_string()
    }

    #[instrument(err)]
    pub async fn play(self, http_port: u16, url_path: &str) -> anyhow::Result<()> {
        let url = self.device.url();
        let host = url.host().ok_or_else(|| {
            error!(%url, "dlna device url doesn't have host");

            anyhow::anyhow!("dlna device url {url} doesn't have host")
        })?;

        info!(host, "get dlna device host done");

        let udp_socket = UdpSocket::bind("0.0.0.0:0")
            .await
            .tap_err(|err| error!(%err, "bind udp socket failed"))?;

        info!("bind udp socket done");

        udp_socket
            .connect(format!("{host}:80"))
            .await
            .tap_err(|err| error!(%err, "udp socket connect failed"))?;

        info!("udp socket connect done");

        let local_ip = udp_socket
            .local_addr()
            .tap_err(|err| error!(%err, "get udp socket local addr failed"))?
            .ip();

        info!(%local_ip, "get udp socket local ip done");

        // we only need what local ip can be connected by dlna device
        drop(udp_socket);

        let payload_setavtransport_uri = format!(
            r#"<InstanceID>0</InstanceID>
        <CurrentURI>http://{local_ip}:{http_port}{}</CurrentURI>
        <CurrentURIMetaData></CurrentURIMetaData>
        "#,
            escape_str_attribute(url_path)
        );

        let service = self
            .device
            .find_service(&AV_TRANSPORT)
            .unwrap_or_else(|| panic!("checked device doesn't have AV_TRANSPORT service"));

        let mut resp = service
            .action(
                url,
                SET_AV_TRANSPORT_URI_ACTION,
                &payload_setavtransport_uri,
            )
            .await
            .tap_err(
                |err| error!(%err, %payload_setavtransport_uri, "set av transport uri failed"),
            )?;

        info!(%payload_setavtransport_uri, "set av transport uri done");
        debug!(?resp, "set av transport uri response");

        resp = service
            .action(url, PLAY_ACTION, PAYLOAD_PLAY)
            .await
            .tap_err(|err| error!(%err, "play video failed"))?;

        info!("play video done");
        debug!(?resp, "play video response");

        Ok(())
    }
}

#[instrument(err)]
pub async fn list_tv(timeout: Duration) -> anyhow::Result<impl Stream<Item = anyhow::Result<TV>>> {
    let device_stream = rupnp::discover(&SearchTarget::URN(AV_TRANSPORT), timeout)
        .await
        .tap_err(|err| error!(%err, "discover TV failed"))?;

    Ok(device_stream
        .try_filter_map(|device| {
            ready(Ok(device
                .find_service(&AV_TRANSPORT)
                .is_some()
                .then_some(TV { device })))
        })
        .map_err(anyhow::Error::from))
}
