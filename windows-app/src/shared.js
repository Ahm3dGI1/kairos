// Helpers every window shares: the IPC call, date formatting, and the small
// amount of DOM building that would otherwise be repeated three times.

export const invoke = (cmd, args) => window.__TAURI__.core.invoke(cmd, args);
export const listen = (event, handler) => window.__TAURI__.event.listen(event, handler);

/** The backend owns "today", so a session open across midnight stays honest. */
export async function today() {
  return parseDate(await invoke('today_date'));
}

/** "2026-09-17" -> a Date at local midnight, avoiding UTC parsing surprises. */
export function parseDate(iso) {
  if (!iso) return null;
  const [y, m, d] = iso.split('-').map(Number);
  return new Date(y, m - 1, d);
}

export function toIso(date) {
  const pad = (n) => String(n).padStart(2, '0');
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

const DAY_MS = 86400000;

/** Days from `today` to `iso`, negative for the past. */
export function daysUntil(iso, todayDate) {
  const date = parseDate(iso);
  if (!date) return null;
  return Math.round((date - todayDate) / DAY_MS);
}

/**
 * A due date the way a person would say it: "Today", "Tomorrow", "Thu 18 Sep",
 * and a year only when it is not this one.
 */
export function formatDue(iso, todayDate) {
  const date = parseDate(iso);
  if (!date) return '';
  const delta = daysUntil(iso, todayDate);
  if (delta === 0) return 'Today';
  if (delta === 1) return 'Tomorrow';
  if (delta === -1) return 'Yesterday';
  if (delta > 1 && delta < 7) return date.toLocaleDateString(undefined, { weekday: 'long' });

  const opts = { day: 'numeric', month: 'short', weekday: 'short' };
  if (date.getFullYear() !== todayDate.getFullYear()) opts.year = 'numeric';
  return date.toLocaleDateString(undefined, opts);
}

/** "17:00" -> "5:00 PM", following the user's locale. */
export function formatTime(hhmm) {
  if (!hhmm) return '';
  const [h, m] = hhmm.split(':').map(Number);
  const date = new Date();
  date.setHours(h, m, 0, 0);
  return date.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' });
}

/** Task.time arrives as "HH:MM:SS"; the UI only ever wants HH:MM. */
export function shortTime(time) {
  return time ? time.slice(0, 5) : '';
}

export function el(tag, props = {}, children = []) {
  const node = document.createElement(tag);
  for (const [key, value] of Object.entries(props)) {
    if (key === 'class') node.className = value;
    else if (key === 'text') node.textContent = value;
    else if (key.startsWith('on')) node.addEventListener(key.slice(2).toLowerCase(), value);
    else if (value !== null && value !== undefined) node.setAttribute(key, value);
  }
  for (const child of [].concat(children)) {
    if (child) node.appendChild(typeof child === 'string' ? document.createTextNode(child) : child);
  }
  return node;
}

export function clear(node) {
  while (node.firstChild) node.removeChild(node.firstChild);
}

/** Priority -> the class suffix used for its dot and border. */
export function priorityClass(priority) {
  return priority && priority !== 'None' ? `p-${String(priority).toLowerCase()}` : '';
}

/**
 * Surfaces an error where the user can see it rather than only in the console.
 * Every command call goes through this, so a failed write is never silent.
 */
export function reportError(where, error) {
  console.error(where, error);
  const bar = document.getElementById('error-bar');
  if (!bar) return;
  bar.textContent = `${where}: ${error}`;
  bar.hidden = false;
  clearTimeout(bar._timer);
  bar._timer = setTimeout(() => {
    bar.hidden = true;
  }, 6000);
}

/** Calls a command, reporting failure instead of throwing into the void. */
export async function call(cmd, args, where = cmd) {
  try {
    return await invoke(cmd, args);
  } catch (error) {
    reportError(where, error);
    return null;
  }
}
