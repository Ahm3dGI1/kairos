import { call, clear, el } from './shared.js';

export function createPrayer({ page, onStatus }) {
  /** The day on screen as an ISO string; null means today. */
  let date = null;
  let data = null;

  async function load() {
    data = await call('prayer_day', { date }, 'Prayer times');
    render();
  }

  function step(delta) {
    const base = data ? new Date(`${data.date}T00:00:00`) : new Date();
    base.setDate(base.getDate() + delta);
    const pad = (n) => String(n).padStart(2, '0');
    date = `${base.getFullYear()}-${pad(base.getMonth() + 1)}-${pad(base.getDate())}`;
    return load();
  }

  function today() {
    date = null;
    return load();
  }

  function render() {
    clear(page);
    if (!data) return;

    onStatus?.(data.label, data.method);

    if (!data.located) {
      page.appendChild(
        el('div', { class: 'prayer-empty' }, [
          el('p', { text: 'Kairos needs to know where you are.' }),
          el('p', {
            class: 'quiet',
            text:
              'Put your latitude and longitude in Settings. Nothing is sent anywhere — ' +
              'the times are worked out from the position of the sun, so the page keeps ' +
              'working with no network at all.',
          }),
        ]),
      );
      return;
    }

    if (data.is_today && data.until_next !== null && data.until_next !== undefined) {
      const next = data.prayers.find((p) => p.is_next);
      if (next) {
        page.appendChild(
          el('div', { class: 'prayer-next' }, [
            el('span', { class: 'label', text: 'NEXT' }),
            el('span', { class: 'name', text: next.name }),
            el('span', { class: 'at', text: next.time }),
            el('span', { class: 'in', text: `in ${describeGap(data.until_next)}` }),
          ]),
        );
      }
    }

    const list = el('div', { class: 'prayer-list' });
    for (const prayer of data.prayers) {
      const classes = ['prayer-row'];
      if (!prayer.is_prayer) classes.push('marker');
      if (prayer.is_next) classes.push('next');
      if (prayer.is_past) classes.push('past');
      if (!prayer.time) classes.push('none');

      list.appendChild(
        el('div', { class: classes.join(' ') }, [
          el('span', { class: 'prayer-name', text: prayer.name }),
          el('span', { class: 'prayer-rule' }),
          el('span', {
            class: 'prayer-time',
            text: prayer.time || 'no angle today',
          }),
        ]),
      );
    }
    page.appendChild(list);

    if (data.no_angle) {
      page.appendChild(
        el('p', { class: 'prayer-note' }, [
          'At this latitude the sun does not go far enough below the horizon ',
          'today for this method to place Fajr or Isha. Umm al-Qura counts ',
          'ninety minutes from Maghrib instead, and does not run out.',
        ]),
      );
    }
  }

  /** "2h 15m", "45m", "now". */
  function describeGap(minutes) {
    if (minutes <= 0) return 'a moment';
    const hours = Math.floor(minutes / 60);
    const rest = minutes % 60;
    if (!hours) return `${rest}m`;
    return rest ? `${hours}h ${rest}m` : `${hours}h`;
  }

  return { load, step, today };
}
