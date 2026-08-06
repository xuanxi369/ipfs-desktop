import { afterEach, describe, expect, it } from "vitest";
import i18n from "./i18n";

describe("language switching", () => {
  afterEach(async () => { await i18n.changeLanguage("en"); });

  it("switches between English and Chinese resources", async () => {
    await i18n.changeLanguage("en");
    const english = i18n.t("dashboard");
    await i18n.changeLanguage("zh");
    const chinese = i18n.t("dashboard");
    expect(i18n.language).toBe("zh");
    expect(chinese).not.toBe(english);
    expect(chinese).not.toBe("dashboard");
  });
});
