import { useState } from "react";
import { t } from "./locale";
/** Decimal editing is an explicit authoring action; untouched rates retain their exact fraction. */
export function decimalRate(value: string) {
  if (!/^\d+(\.\d{1,6})?$/.test(value)) throw Error("Enter a positive number with up to six decimal places.");
  const [whole, fraction = ""] = value.split(".");
  let numerator = BigInt(whole + fraction), denominator = 10n ** BigInt(fraction.length);
  const gcd = (a: bigint, b: bigint): bigint => b ? gcd(b, a % b) : a;
  const divisor = gcd(numerator, denominator); numerator /= divisor; denominator /= divisor;
  if (numerator < 1 || numerator > 4294967295n || denominator > 4294967295n) throw Error("Enter a positive number within the supported range.");
  return [String(numerator), String(denominator)] as const;
}
export function RateInput({ numerator, denominator, onChange, frameRate = false }: { numerator: string; denominator: string; onChange(n: string, d: string): void; frameRate?: boolean }) {
  const [draft, setDraft] = useState(() => String(Number((Number(numerator) / Number(denominator)).toFixed(6))));
  const [invalid, setInvalid] = useState(false);
  return <label>{t(frameRate ? "Frame rate (fps)" : "Playback speed (×)")}<input inputMode="decimal" value={draft} aria-invalid={invalid || undefined} onChange={e => {
    const value = e.target.value; setDraft(value);
    try { const rate = frameRate && ["23.976", "29.97", "59.94"].includes(value) ? [String(({"23.976":24000,"29.97":30000,"59.94":60000} as Record<string,number>)[value]), "1001"] : decimalRate(value); onChange(rate[0], rate[1]); setInvalid(false); }
    catch { onChange("", ""); setInvalid(true); }
  }} />{invalid && <span role="alert">{t("Enter a positive number.")}</span>}</label>;
}
