// The habit month, Direction B.
//
// A contribution grid: one row per habit, one square per day, filled when done
// and hollow-red when missed. A month is the right window because a habit is a
// question about consistency, and consistency is invisible one day at a time.
//
// Under it, the month's journal as a log — date on the left, the line on the
// right — and a summary of how the month actually went.

import { call, clear, el, parseDate, toIso } from './shared.js';

export function createHabits({ rows, journal, summary, getToday, onMonth }) {
  /** The month on screen as [year, month]; null means the current one. */
  let shown = null;
  let data = null;

  async function load() {
    if (!shown) {
      shown = (await call('current_month_pair', undefined, 'Month')) ?? null;
      if (!shown) return;
    }
    const [year, month] = shown;
    data = await call('habit_month', { year, month }, 'Habits');
    if (!data) return;

    onMonth?.(data.label, data.habits.length);
    renderGrid();
    await renderJournal();
  }

  function step(delta) {
    const [year, month] = shown ?? [getToday().getFullYear(), getToday().getMonth() + 1];
    const date = new Date(year, month - 1 + delta, 1);
    shown = [date.getFullYear(), date.getMonth() + 1];
    load();
  }

  // ---------- the grid ----------

  function renderGrid() {
    clear(rows);

    // A ruler rather than a label per day: every fifth number, dots between.
    const ruler = el('div', { class: 'habit-row' }, [
      el('div', { class: 'habit-name' }),
      el('div', { class: 'day-nums' }),
    ]);
    const nums = ruler.querySelector('.day-nums');
    for (const day of data.days) {
      const mark = day.day === 1 || day.day % 5 === 0;
      nums.appendChild(
        el('span', {
          class: day.is_today ? 'today' : mark ? 'mark' : '',
          text: day.is_today || mark ? String(day.day) : '·',
        }),
      );
    }
    rows.appendChild(ruler);

    for (const habit of data.habits) {
      rows.appendChild(habitRow(habit));
    }

    if (!data.habits.length) {
      rows.appendChild(
        el('div', { class: 'empty', text: 'no habits yet — type one below and press Enter' }),
      );
    }
  }

  function habitRow(habit) {
    const name = el('span', { class: 'n', text: habit.name, title: 'Double-click to rename' });
    name.addEventListener('dblclick', () => {
      const input = el('input', { class: 'n', type: 'text', value: habit.name });
      input.style.cssText =
        'border:0;background:transparent;outline:none;font:inherit;color:inherit;width:100%';
      name.replaceWith(input);
      input.focus();
      input.select();
      const commit = async () => {
        await call('rename_habit', { id: habit.id, name: input.value }, 'Rename habit');
        load();
      };
      input.addEventListener('blur', commit);
      input.addEventListener('keydown', (event) => {
        if (event.key === 'Enter') commit();
        if (event.key === 'Escape') load();
      });
    });

    // The stats belong on the right, where every row's read the same way.
    const elapsed = data.days.filter((d) => !d.is_future).length;
    const stats = habit.numeric
      ? [habit.average ?? '—', `${habit.count}/${elapsed}`]
      : [`${habit.count}/${elapsed}`];
    if (!habit.numeric && habit.streak > 0) stats.push(`${habit.streak}d streak`);

    const days = el('div', { class: 'habit-days' });
    if (habit.numeric) {
      numericDays(habit, days);
      return el('div', { class: 'habit-row' }, [
        el('div', { class: 'habit-name' }, [name, kindTag(habit)]),
        days,
        el('span', { class: 'habit-total', text: stats.join(' · ') }),
      ]);
    }

    habit.done.forEach((done, index) => {
      const day = data.days[index];
      const classes = ['day'];
      if (done) classes.push('on');
      else if (day.is_future) classes.push('ahead');
      else classes.push('miss');

      const cell = el('button', {
        type: 'button',
        class: classes.join(' '),
        title: `${habit.name} · ${day.date}${done ? ' · done' : ''}`,
        'aria-label': `${habit.name} on ${day.date}`,
        disabled: day.is_future ? '' : null,
      });
      if (!day.is_future) {
        cell.addEventListener('click', async () => {
          await call('toggle_habit', { id: habit.id, date: day.date }, 'Habit');
          load();
        });
      }
      days.appendChild(cell);
    });

    return el('div', { class: 'habit-row' }, [
      el('div', { class: 'habit-name' }, [name]),
      days,
      el('span', { class: 'habit-total', text: stats.join(' · ') }),
    ]);
  }

  /** The unit, so a row of bare blocks still says what it counts. */
  function kindTag(habit) {
    const unit = habit.kind.startsWith('number:')
      ? habit.kind.slice('number:'.length)
      : habit.kind === 'duration'
        ? 'h/m'
        : '';
    return unit ? el('span', { class: 'sub', text: unit }) : null;
  }

  /**
   * A numeric row on the same 31-column grid as every other.
   *
   * A cell is too narrow for "7h 30m", so the value is shown as an intensity —
   * darkest is the month's largest — and read exactly on hover. Clicking opens
   * an input over the cell, which is the only place wide enough for one.
   */
  function numericDays(habit, days) {
    const numbers = habit.values.map(parseLeadingNumber);
    const top = Math.max(...numbers.filter((n) => n !== null), 0);

    habit.values.forEach((text, index) => {
      const day = data.days[index];
      const value = numbers[index];
      const classes = ['day', 'num'];
      if (day.is_future) classes.push('ahead');
      else if (value === null) classes.push('blank');

      const cell = el('button', {
        type: 'button',
        class: classes.join(' '),
        title: `${habit.name} · ${day.date}${text ? ` · ${text}` : ''}`,
        'aria-label': `${habit.name} on ${day.date}${text ? `: ${text}` : ''}`,
        disabled: day.is_future ? '' : null,
      });
      if (value !== null && top > 0) {
        // A floor of 0.18 so the smallest recorded day is still visible as a
        // day that recorded something, rather than as a blank.
        cell.style.opacity = String(0.18 + 0.82 * (value / top));
        cell.classList.add('on');
      }
      if (!day.is_future) {
        cell.addEventListener('click', () => editCell(habit, day, text, cell, days));
      }
      days.appendChild(cell);
    });
  }

  /** An input floated over one cell, because a 19px cell cannot hold one. */
  function editCell(habit, day, current, cell, days) {
    days.querySelector('.cell-input')?.remove();

    const input = el('input', {
      class: 'cell-input',
      type: 'text',
      value: current ?? '',
      spellcheck: 'false',
      placeholder: habit.kind === 'duration' ? '7h 30m' : '0',
    });
    input.style.left = `${cell.offsetLeft}px`;
    days.appendChild(input);
    input.focus();
    input.select();

    let closed = false;
    const commit = async (save) => {
      if (closed) return;
      closed = true;
      const value = input.value;
      input.remove();
      if (!save) return;
      await call('set_habit_value', { date: day.date, id: habit.id, value }, 'Habit value');
      load();
    };
    input.addEventListener('blur', () => commit(true));
    input.addEventListener('keydown', (event) => {
      if (event.key === 'Enter') commit(true);
      if (event.key === 'Escape') {
        event.stopPropagation();
        commit(false);
      }
    });
  }

  /** "7h 30m" and "42 pages" both start with the number that matters. */
  function parseLeadingNumber(text) {
    if (!text) return null;
    const hours = /^(\d+(?:\.\d+)?)h(?:\s*(\d+)m?)?$/i.exec(text.trim());
    if (hours) return Number(hours[1]) * 60 + Number(hours[2] ?? 0);
    const minutes = /^(\d+)m$/i.exec(text.trim());
    if (minutes) return Number(minutes[1]);
    const plain = parseFloat(text);
    return Number.isFinite(plain) ? plain : null;
  }

  /** Space on the habits page ticks every habit for today — the common case. */
  async function toggleToday() {
    if (!data?.habits.length) return;
    const todayIso = toIso(getToday());
    const index = data.days.findIndex((d) => d.date === todayIso);
    if (index < 0) return;

    // If any is untouched, fill them in; if all are done, clear them.
    const allDone = data.habits.every((h) => h.done[index]);
    for (const habit of data.habits) {
      if (habit.done[index] === !allDone) continue;
      await call('toggle_habit', { id: habit.id, date: todayIso }, 'Habit');
    }
    load();
  }

  // ---------- the journal ----------

  async function renderJournal() {
    const [year, month] = shown;
    const page = await call('month_journal', { year, month }, 'Journal');
    clear(journal);
    clear(summary);

    const entries = page?.entries ?? [];
    if (!entries.length) {
      journal.appendChild(
        el('div', { class: 'empty', text: 'nothing written this month' }),
      );
    }
    const todayIso = toIso(getToday());
    for (const entry of entries) {
      journal.appendChild(entryRow(year, month, entry, todayIso));
    }

    renderSummary();
  }

  function entryRow(year, month, entry, todayIso) {
    const body = el('textarea', { class: 'what', rows: '1' });
    body.value = entry.body;
    const size = () => {
      body.style.height = 'auto';
      body.style.height = `${body.scrollHeight}px`;
    };
    body.addEventListener('input', size);

    let timer = null;
    const save = () =>
      call(
        'save_journal_entry',
        { year, month, entry: { id: entry.id, title: entry.title, body: body.value } },
        'Journal',
      );
    body.addEventListener('input', () => {
      clearTimeout(timer);
      timer = setTimeout(save, 600);
    });
    body.addEventListener('blur', save);

    // An entry headed with today's date is the live one.
    const isToday = entry.title && parseDate(todayIso)?.getDate() === Number(entry.title.match(/\d+/)?.[0]);
    const row = el('div', { class: `entry ${isToday ? 'today' : ''}`.trim() }, [
      el('span', { class: 'when', text: entry.title || '—' }),
      body,
    ]);
    requestAnimationFrame(size);
    return row;
  }

  /** How the month actually went, in four numbers. */
  function renderSummary() {
    const past = data.days.filter((d) => !d.is_future).length;
    const slots = past * data.habits.length;
    const done = data.habits.reduce((sum, h) => sum + h.count, 0);
    const pct = slots ? Math.round((done / slots) * 100) : 0;
    const best = data.habits.reduce((max, h) => Math.max(max, h.streak), 0);
    const perfect = data.days.filter(
      (day, i) => !day.is_future && data.habits.length && data.habits.every((h) => h.done[i]),
    ).length;

    summary.append(
      el('span', { class: 'block-head' }, [el('span', { text: 'THIS MONTH' })]),
      el('div', { style: 'display:flex;align-items:baseline;gap:8px' }, [
        el('span', { class: 'big', text: `${pct}%` }),
        el('span', { style: 'font-family:var(--mono);font-size:10px;color:var(--text-fainter)', text: 'completion' }),
      ]),
      el('div', { class: 'rows' }, [
        el('div', {}, [el('span', { text: 'best streak' }), el('b', { text: `${best}d` })]),
        el('div', {}, [el('span', { text: 'perfect days' }), el('b', { text: String(perfect) })]),
        el('div', {}, [
          el('span', { text: 'missed' }),
          el('b', { class: 'bad', text: String(Math.max(0, slots - done)) }),
        ]),
      ]),
      el('div', { class: 'legend' }, [
        el('span', {}, [
          el('span', { class: 'swatch', style: 'background:var(--habit-done)' }),
          document.createTextNode('done'),
        ]),
        el('span', {}, [
          el('span', {
            class: 'swatch',
            style: 'background:var(--habit-miss);border:1px solid var(--habit-miss-line)',
          }),
          document.createTextNode('missed'),
        ]),
      ]),
    );
  }

  /** Adds a habit, or a journal line for today, from the shared capture bar. */
  async function submit(text) {
    const line = text.trim();
    if (!line) return;
    const [year, month] = shown ?? [];
    if (!year) return;

    const today = getToday();
    const heading = today.toLocaleDateString(undefined, { weekday: 'short', day: 'numeric' });
    await call(
      'save_journal_entry',
      { year, month, entry: { id: null, title: heading, body: line } },
      'Journal',
    );
    load();
  }

  /**
   * Adds a habit, optionally saying what it records: "Sleep :duration",
   * "Pages :number pages". Bare means a tick, which is the common case.
   */
  async function addHabit(line) {
    const [name, ...rest] = String(line).split(':');
    const spec = rest.join(':').trim();
    const kind = spec
      ? spec.startsWith('number')
        ? `number:${spec.slice('number'.length).trim()}`.replace(/:$/, '')
        : spec.split(/\s+/)[0]
      : null;
    return addHabitNamed(name.trim(), kind);
  }

  async function addHabitNamed(name, kind) {
    if (!name) return;
    await call('add_habit', { name, kind }, 'Add habit');
    load();
  }

  return { load, step, toggleToday, submit, addHabit };
}
