import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import { formatError, shortHash } from "./types";
import { Icon } from "./Icons";

interface PeerGeoPoint { peer_id: string; country: string; country_code: string; region: string; city: string; latitude: number; longitude: number; }
interface PeerGeoReport { connected_peers: number; public_addresses: number; located_peers: number; countries: Record<string, number>; points: PeerGeoPoint[]; }

export default function PeerMap({ isRunning, setError }: { isRunning: boolean; setError: (e: string) => void }) {
  const { t } = useTranslation();
  const [report, setReport] = useState<PeerGeoReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [selected, setSelected] = useState<PeerGeoPoint | null>(null);
  async function refresh() {
    try {
      setLoading(true);
      const raw = await invoke<Partial<PeerGeoReport>>("get_peer_geography");
      const points = Array.isArray(raw?.points) ? raw.points.filter((point): point is PeerGeoPoint =>
        !!point && Number.isFinite(Number(point.latitude)) && Number.isFinite(Number(point.longitude))) : [];
      setSelected(null);
      setReport({
        connected_peers: Number(raw?.connected_peers) || 0,
        public_addresses: Number(raw?.public_addresses) || 0,
        located_peers: points.length,
        countries: raw?.countries && typeof raw.countries === "object" ? raw.countries : {},
        points,
      });
      setError("");
    }
    catch (e) { setError(`${t("peerMap")}: ${formatError(e)}`); }
    finally { setLoading(false); }
  }
  const countries = Object.entries(report?.countries ?? {}).sort((a, b) => b[1] - a[1]);
  return <section className="peer-map-card">
    <div className="section-header"><div><span className="section-kicker">NETWORK MAP</span><h2>{t("peerMap")}</h2><p className="section-description">{t("peerMapDescription")}</p></div><button className="btn-small btn-download" onClick={refresh} disabled={loading}>{loading ? <span className="spinner"/> : <Icon name="globe"/>} {t("refresh")}</button></div>
    {report ? <>
      <div className="peer-map-summary"><span><strong>{report.connected_peers}</strong>{t("connectedPeers")}</span><span><strong>{report.located_peers}</strong>{t("locatedPeers")}</span><span><strong>{countries.length}</strong>{t("countriesRegions")}</span></div>
      <div className="peer-map-layout">
        <div className="peer-world-map" role="img" aria-label={t("peerMap")}>
          <svg className="peer-map-base" viewBox="0 0 1000 500" preserveAspectRatio="none" aria-hidden="true">
            <defs><linearGradient id="mapSea" x1="0" y1="0" x2="1" y2="1"><stop stopColor="#102d3a"/><stop offset="1" stopColor="#071923"/></linearGradient><pattern id="mapGrid" width="62.5" height="62.5" patternUnits="userSpaceOnUse"><path d="M62.5 0H0V62.5" fill="none" stroke="#4dc6c0" strokeOpacity=".09"/></pattern></defs>
            <rect width="1000" height="500" fill="url(#mapSea)"/><rect width="1000" height="500" fill="url(#mapGrid)"/>
            <g className="peer-map-land">
              <path d="M42 94 68 65 111 48 160 51 194 38 235 57 260 84 252 111 229 126 221 151 191 162 170 190 145 207 116 190 105 160 77 151 60 127Z"/>
              <path d="M185 211 217 220 241 246 252 280 244 318 223 361 205 404 182 389 171 350 154 319 155 282 168 251Z"/>
              <path d="M405 83 435 60 477 56 502 73 533 70 561 88 590 85 618 66 661 64 690 79 727 80 758 101 800 108 830 132 821 157 789 167 770 188 738 185 715 205 680 197 652 181 624 189 600 173 570 176 545 157 516 164 493 146 465 149 444 132 416 125 392 104Z"/>
              <path d="M455 168 493 174 519 197 536 230 525 265 508 296 487 330 463 310 449 279 432 248 435 211Z"/>
              <path d="M802 284 830 267 867 273 894 299 888 330 859 348 824 339 795 313Z"/>
              <path d="M888 183 902 172 915 183 908 204 894 212 883 198Z"/>
              <path d="M344 91 360 82 375 91 369 111 350 114 338 103Z"/>
              <path d="M250 438 304 447 367 451 427 449 486 458 548 451 611 456 674 447 735 451 790 440 759 468 681 479 596 474 510 481 424 475 335 481 270 468Z"/>
            </g>
          </svg>
          {report.points.map((point, i) => {
            const lat = Math.max(-85, Math.min(85, point.latitude)) * Math.PI / 180;
            const x = ((point.longitude + 180) / 360) * 100;
            const y = (0.5 - Math.log((1 + Math.sin(lat)) / (1 - Math.sin(lat))) / (4 * Math.PI)) * 100;
            return <button key={`${point.peer_id}-${i}`} className={`peer-dot ${selected === point ? "selected" : ""}`} style={{ left: `${x}%`, top: `${y}%` }} onClick={() => setSelected(point)} aria-label={`${point.city || point.region}, ${point.country}`}><span/></button>;
          })}
          {selected && <div className="peer-map-tooltip"><strong>{selected.city || selected.region || selected.country}</strong><span>{selected.country} · {shortHash(selected.peer_id, 7)}</span></div>}
        </div>
        <div className="peer-country-list"><h3>{t("peerRegions")}</h3>{countries.length ? countries.slice(0, 12).map(([country, count]) => <div key={country}><span>{country || t("unknown")}</span><strong>{count}</strong></div>) : <p className="section-description">{t("noPublicPeerLocations")}</p>}</div>
      </div>
      <p className="peer-map-note">{t("peerMapPrivacy")}</p>
    </> : <div className="empty-state">{isRunning ? t("loadPeerMap") : t("peerMapOfflineHint")}</div>}
  </section>;
}
