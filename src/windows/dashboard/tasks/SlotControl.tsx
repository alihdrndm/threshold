import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
} from "react";
import type { Task } from "@/lib/tauri";
import { Popover } from "./Popover";
import { formatSlot, fromLocalInput, toLocalInput } from "./slot";

/**
 * The calendar's reach onto a card, without threading commands through five
 * components. Mirrors AreasContext: the value is the connected flag and the
 * verbs a card needs, and every verb reports its own failure to the error line
 * the dashboard already shows.
 */
export const CalendarContext = createContext<{
  connected: boolean;
  reschedule: (task: Task, startTs: number | null) => void;
  remove: (task: Task) => void;
  open: (url: string) => void;
} | null>(null);

/**
 * The slot on a Schedule card: the time it holds, or "no date yet", and a menu
 * to change it. Shown only for tasks in Schedule (the caller gates that). Same
 * eyebrow vocabulary as the area label; the same portalled Popover as the area
 * menu, so it can never hide behind another card.
 */
export function SlotControl({ task }: { task: Task }) {
  const cal = useContext(CalendarContext);
  const [open, setOpen] = useState(false);
  const [picking, setPicking] = useState(false);
  const button = useRef<HTMLButtonElement>(null);
  const close = useCallback(() => {
    setOpen(false);
    setPicking(false);
  }, []);

  if (!cal) return null;

  const label = cal.connected ? formatSlot(task.scheduledTs, new Date()) : "connect calendar";

  return (
    <>
      <button
        ref={button}
        type="button"
        onClick={() => setOpen((o) => !o)}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label={`Scheduled: ${label}. Change`}
        className="grid h-6 shrink-0 place-items-center rounded-full px-2 text-[10px] tracking-[0.14em] text-[var(--zone-ink-muted)] uppercase transition duration-150 hover:text-[var(--color-ink)] active:scale-95"
      >
        {label}
      </button>

      {open && button.current && (
        <Popover anchor={button.current} role="menu" onClose={close}>
          {!cal.connected ? (
            <Item muted onPress={close}>
              Connect Google Calendar in Settings
            </Item>
          ) : picking ? (
            <PickTime
              initial={toLocalInput(task.scheduledTs)}
              onCommit={(ts) => cal.reschedule(task, ts)}
              onDone={close}
            />
          ) : (
            <>
              <Item
                onPress={() => {
                  close();
                  cal.reschedule(task, null);
                }}
              >
                Move to next free slot
              </Item>
              <Item onPress={() => setPicking(true)}>Pick a time…</Item>
              {task.calendarHtmlLink && (
                <Item
                  onPress={() => {
                    close();
                    cal.open(task.calendarHtmlLink!);
                  }}
                >
                  Open in Google Calendar
                </Item>
              )}
              <Item
                muted
                onPress={() => {
                  close();
                  cal.remove(task);
                }}
              >
                Remove from calendar
              </Item>
            </>
          )}
        </Popover>
      )}
    </>
  );
}

/** How long a pick may rest before it saves itself. Long enough to arrow
    through hours without a save per step; short enough that the answer
    arrives while the eye is still on the panel. */
const SETTLE_MS = 700;

/**
 * The "pick a time" panel. There is no button: every complete pick commits
 * itself once it settles, Enter commits at once and closes, and a change
 * still pending when the popover closes is flushed on the way out. The
 * caption is the whole answer — "saves as you pick" until the first save,
 * then the saved slot in the card's own words, so the panel teaches the
 * label the card is about to show.
 */
function PickTime({
  initial,
  onCommit,
  onDone,
}: {
  initial: string;
  onCommit: (ts: number) => void;
  onDone: () => void;
}) {
  const [value, setValue] = useState(initial);
  const [saved, setSaved] = useState(false);
  const timer = useRef<number | undefined>(undefined);
  // What the calendar already holds. Commits compare against this so closing
  // the panel untouched, or settling on the original time, sends nothing.
  const sent = useRef(initial);
  const pending = useRef(initial);

  const commit = useCallback(
    (v: string) => {
      const ts = fromLocalInput(v);
      if (ts === null || v === sent.current) return false;
      sent.current = v;
      onCommit(ts);
      return true;
    },
    [onCommit],
  );

  // The flush: whatever is still resting when the panel unmounts - outside
  // click, Escape, the menu closing under it - is saved on the way out.
  useEffect(() => {
    return () => {
      window.clearTimeout(timer.current);
      commit(pending.current);
    };
  }, [commit]);

  return (
    <li role="none" className="slot-picker flex w-52 flex-col gap-1.5 p-1">
      <span className="px-0.5 text-[10px] tracking-[0.14em] text-[var(--color-ink-muted)] uppercase">
        Pick a time
      </span>
      <input
        type="datetime-local"
        autoFocus
        value={value}
        onChange={(event) => {
          const v = event.target.value;
          setValue(v);
          pending.current = v;
          setSaved(false);
          window.clearTimeout(timer.current);
          timer.current = window.setTimeout(() => {
            if (commit(v)) setSaved(true);
          }, SETTLE_MS);
        }}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            window.clearTimeout(timer.current);
            commit(pending.current);
            onDone();
          }
        }}
        className="w-full rounded-lg border border-[var(--color-border-subtle)] bg-[var(--color-fill-subtle)] px-2.5 py-2 text-sm text-[var(--color-ink)] outline-none"
        aria-label="Pick a date and time. Saves as you pick"
      />
      <p
        key={saved ? `saved-${sent.current}` : "hint"}
        aria-live="polite"
        className="slot-caption flex min-h-4 items-center gap-1.5 px-0.5 text-[11px] text-[var(--color-ink-muted)]"
      >
        {saved ? (
          <>
            <span className="slot-saved-dot shrink-0" aria-hidden />
            Saved · {formatSlot(fromLocalInput(sent.current), new Date())}
          </>
        ) : (
          "Saves as you pick"
        )}
      </p>
    </li>
  );
}

function Item({
  children,
  muted = false,
  onPress,
}: {
  children: React.ReactNode;
  muted?: boolean;
  onPress: () => void;
}) {
  return (
    <li role="none">
      <button
        type="button"
        role="menuitem"
        onClick={onPress}
        className={
          "w-full rounded-lg px-2.5 py-1.5 text-left transition-colors duration-100 hover:bg-[var(--color-fill-selected)] " +
          (muted ? "text-[var(--color-ink-muted)]" : "text-[var(--color-ink)]")
        }
      >
        {children}
      </button>
    </li>
  );
}
