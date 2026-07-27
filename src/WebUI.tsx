import { useTranslation } from "react-i18next";

interface WebUIProps {
  webuiUrl: string;
  openWebui: () => Promise<void>;
}

export default function WebUI({ webuiUrl, openWebui }: WebUIProps) {
  const { t } = useTranslation();

  return (
    <div className="webui-container">
      <div className="webui-toolbar">
        <span>IPFS WebUI</span>
        <button onClick={openWebui} className="btn-small">{t("openBrowser")}</button>
      </div>
      <iframe src={webuiUrl} className="webui-iframe" title="IPFS WebUI" sandbox="allow-scripts allow-same-origin allow-forms" />
    </div>
  );
}
