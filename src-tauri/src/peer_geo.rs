use crate::daemon::IpfsApiClient;
use crate::error::DaemonError;
use futures_util::{stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::time::Duration;

const MAX_GEO_PEERS: usize = 80;
const GEO_CONCURRENCY: usize = 6;

#[derive(Debug, Clone, Serialize)]
pub struct PeerGeoPoint {
    pub peer_id: String,
    pub country: String,
    pub country_code: String,
    pub region: String,
    pub city: String,
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PeerGeoReport {
    pub connected_peers: usize,
    pub public_addresses: usize,
    pub located_peers: usize,
    pub countries: HashMap<String, usize>,
    pub points: Vec<PeerGeoPoint>,
}

#[derive(Debug, Deserialize)]
struct GeoResponse {
    success: bool,
    #[serde(default)]
    country: String,
    #[serde(default)]
    country_code: String,
    #[serde(default)]
    region: String,
    #[serde(default)]
    city: String,
    latitude: Option<f64>,
    longitude: Option<f64>,
}

fn public_ip_from_multiaddr(addr: &str) -> Option<IpAddr> {
    let parts: Vec<&str> = addr.split('/').collect();
    let raw = parts.windows(2).find_map(|pair| match pair[0] {
        "ip4" | "ip6" => Some(pair[1]),
        _ => None,
    })?;
    let ip: IpAddr = raw.parse().ok()?;
    let public = match ip {
        IpAddr::V4(v4) => {
            !v4.is_private()
                && !v4.is_loopback()
                && !v4.is_link_local()
                && !v4.is_broadcast()
                && !v4.is_documentation()
                && !v4.is_unspecified()
        }
        IpAddr::V6(v6) => {
            !v6.is_loopback()
                && !v6.is_unspecified()
                && !v6.is_unique_local()
                && !v6.is_unicast_link_local()
        }
    };
    public.then_some(ip)
}

pub async fn locate_connected_peers(api: &IpfsApiClient) -> Result<PeerGeoReport, DaemonError> {
    let peers = api.swarm_peers().await?;
    let connected_peers = peers.peers.len();
    let mut seen = HashSet::new();
    let candidates: Vec<(String, IpAddr)> = peers
        .peers
        .into_iter()
        .filter_map(|peer| public_ip_from_multiaddr(&peer.addr).map(|ip| (peer.peer, ip)))
        .filter(|(_, ip)| seen.insert(*ip))
        .take(MAX_GEO_PEERS)
        .collect();
    let public_addresses = candidates.len();

    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(6))
        .build()
        .map_err(|e| DaemonError::ApiError(format!("GeoIP client: {e}")))?;

    let points: Vec<PeerGeoPoint> = stream::iter(candidates)
        .map(|(peer_id, ip)| {
            let http = http.clone();
            async move {
                let url = format!("https://ipwho.is/{ip}");
                let geo = http
                    .get(url)
                    .send()
                    .await
                    .ok()?
                    .json::<GeoResponse>()
                    .await
                    .ok()?;
                if !geo.success {
                    return None;
                }
                Some(PeerGeoPoint {
                    peer_id,
                    country: geo.country,
                    country_code: geo.country_code,
                    region: geo.region,
                    city: geo.city,
                    latitude: geo.latitude?,
                    longitude: geo.longitude?,
                })
            }
        })
        .buffer_unordered(GEO_CONCURRENCY)
        .filter_map(|point| async move { point })
        .collect()
        .await;

    let mut countries = HashMap::new();
    for point in &points {
        *countries.entry(point.country.clone()).or_insert(0) += 1;
    }

    Ok(PeerGeoReport {
        connected_peers,
        public_addresses,
        located_peers: points.len(),
        countries,
        points,
    })
}

#[cfg(test)]
mod tests {
    use super::public_ip_from_multiaddr;

    #[test]
    fn extracts_public_ip() {
        assert!(public_ip_from_multiaddr("/ip4/8.8.8.8/tcp/4001").is_some());
        assert!(public_ip_from_multiaddr("/ip4/192.168.1.2/tcp/4001").is_none());
        assert!(public_ip_from_multiaddr("/dns4/example.com/tcp/4001").is_none());
    }
}
