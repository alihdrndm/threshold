import { createContext, useCallback, useContext, useRef, useState } from "react";
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
            <li role="none" className="p-1">
              {/* A native picker: no dependency, keyboard-accessible, and it
                  speaks the user's locale. Enter or the button commits. */}
              <input
                type="datetime-local"
                autoFocus
                defaultValue={toLocalInput(task.scheduledTs)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    const ts = fromLocalInput((event.target as HTMLInputElement).value);
                    if (ts !== null) cal.reschedule(task, ts);
                    close();
                  }
                }}
                className="w-full rounded-lg border border-[var(--color-border-subtle)] bg-[var(--color-fill-subtle)] px-2 py-1.5 text-sm text-[var(--color-ink)] outline-none"
                aria-label="Pick a date and time"
              />
            </li>
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
