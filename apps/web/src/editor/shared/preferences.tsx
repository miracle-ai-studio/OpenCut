import { Moon, Sun } from "lucide-react";
import { useLocale, preferences } from "./locale";
import { Button } from "./ui";
export function Preferences() {
  const { language, theme, t } = useLocale();
  return (
    <div className="preference-controls">
      <select
        aria-label={t("Language")}
        value={language}
        onChange={(event) =>
          preferences({ language: event.target.value as "zh" | "en" })
        }
      >
        <option value="zh">简体中文</option>
        <option value="en">English</option>
      </select>
      <Button
        title={t(theme === "dark" ? "Light theme" : "Dark theme")}
        aria-label={t(theme === "dark" ? "Light theme" : "Dark theme")}
        onClick={() =>
          preferences({ theme: theme === "dark" ? "light" : "dark" })
        }
        icon={theme === "dark" ? <Sun size={17} /> : <Moon size={17} />}
      />
    </div>
  );
}
