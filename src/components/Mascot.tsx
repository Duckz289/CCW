import { useEffect, useMemo, useState } from "react";
import normalFrame from "../../action/NORMAL.png";
import alertDownFrame from "../../action/ALERT/DOWN.png";
import alertUpFrame from "../../action/ALERT/UP.png";
import blinkCloseFrame from "../../action/OPEN_CLOSE_EYES/CLOSE.png";
import blinkOpenFrame from "../../action/OPEN_CLOSE_EYES/OPEN.png";
import binEndFrame from "../../action/THROW_TRASH/BIN_FOLDER_END.png";
import binNoFrame from "../../action/THROW_TRASH/BIN_NO.png";
import binNormalFrame from "../../action/THROW_TRASH/BIN_NOR.png";
import binFolderFrame from "../../action/THROW_TRASH/BIN_W_FOLDER.png";
import type { Language, translations } from "../i18n";

interface MascotProps {
  alertActive: boolean;
  cleaning: boolean;
  copy: (typeof translations)[Language]["mascot"];
}

const IDLE_FRAMES = [normalFrame, blinkOpenFrame, blinkCloseFrame, blinkOpenFrame];
const ALERT_FRAMES = [alertUpFrame, alertDownFrame];
const THROW_FRAMES = [binNormalFrame, binNoFrame, binFolderFrame, binEndFrame];

export function Mascot({ alertActive, cleaning, copy }: MascotProps) {
  const [frameIndex, setFrameIndex] = useState(0);

  const mode = cleaning ? "throw" : alertActive ? "alert" : "idle";
  const frames = useMemo(() => {
    if (mode === "throw") return THROW_FRAMES;
    if (mode === "alert") return ALERT_FRAMES;
    return IDLE_FRAMES;
  }, [mode]);

  useEffect(() => {
    setFrameIndex(0);

    if (mode === "throw") {
      const timer = window.setInterval(() => {
        setFrameIndex((current) => {
          if (current >= THROW_FRAMES.length - 1) {
            window.clearInterval(timer);
            return current;
          }
          return current + 1;
        });
      }, 135);

      return () => window.clearInterval(timer);
    }

    const timer = window.setInterval(
      () => {
        setFrameIndex((current) => (current + 1) % frames.length);
      },
      mode === "alert" ? 300 : 520,
    );

    return () => window.clearInterval(timer);
  }, [frames.length, mode]);

  const label = mode === "throw" ? copy.cleaning : mode === "alert" ? copy.alert : copy.idle;
  const detail = mode === "throw" ? copy.cleaningDetail : alertActive ? copy.alertDetail : copy.idleDetail;
  const ariaLabel = mode === "throw" ? copy.cleaningAria : mode === "alert" ? copy.alertAria : copy.idleAria;

  return (
    <figure className={`mascot-card ${mode === "alert" ? "mascot-card-alert" : ""}`} aria-label={ariaLabel}>
      <div className="mascot-glade">
        <img className="mascot-sprite" src={frames[frameIndex]} alt="" draggable={false} />
      </div>
      <figcaption className="mt-4 text-center">
        <p className="text-sm font-semibold text-text">{label}</p>
        <p className="mt-1 text-xs text-muted">{detail}</p>
      </figcaption>
    </figure>
  );
}
