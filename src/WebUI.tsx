import { useTranslation } from "react-i18next";

interface WebUIProps {
  webuiUrl: string;
  openWebui: () => Promise<void>;
}

export default function WebUI({ webuiUrl, openWebui }: WebUIProps) {
  const { t } = useTranslation();

  return (
    <div className="webui-page">
      <div className="page-intro"><div><span className="section-kicker">ADVANCED</span><h2>Web UI</h2><p>{t("webuiDescription")}</p></div></div>
      <div className="webui-container">
      <div className="webui-toolbar">
        <span>IPFS WebUI</span>
        <button onClick={openWebui} className="btn-small">{t("openBrowser")}</button>
      </div>
        <iframe src={webuiUrl} className="webui-iframe" title="IPFS WebUI" />
      </div>
    </div>
  );
}
