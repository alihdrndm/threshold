import { Ritual } from "./ritual/Ritual";

/**
 * The fullscreen intention ritual (spec F1). The trigger that opened it comes
 * through the URL so the record can say whether this was a boot, a wake or an
 * unlock.
 */
export function PopupWindow() {
  const trigger =
    new URLSearchParams(window.location.search).get("trigger") ?? "manual";

  return <Ritual trigger={trigger} />;
}
