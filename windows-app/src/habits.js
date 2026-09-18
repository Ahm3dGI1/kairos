// The habit month.
//
// One month at a time: habits down the side, days across the top, a tick in
// each cell. A month is the right window because a habit is a question about
// consistency, and you cannot see consistency one day at a time.
//
// The journal beside it belongs to the whole month, not to any single day —
// entries carry their own headings, so a day, a week or a trip can each be one
// block.

import { call, clear, el, toIso } from './shared.js';

const DURATION_HINT = '7h30, 7.5h or 450';

export function createHabitMonth({ grid, newHabit, entriesNode, addEntry, label, getToday }) {
  /** The month on screen, as [year, month]; null means the current one. */
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
    label.textContent = data.label;
    renderGrid();
    await renderJournal();
  }

  function step(delta) {
    const [year, month] = shown;
    const date = new Date(year, month - 1 + delta, 1);
    shown = [date.getFullYear(), date.getMonth() + 1];
    load();
  }

  function thisMonth() {
    shown = null;
    load();
  }

  // ---------- the grid ----------

  function renderGrid() {
    clear(grid);
    // One column for the name, one per day, one for the row total.
    grid.style.setProperty('--days', String(data.days.length));

    grid.appendChild(el('div', { class: 'cell corner', text: '' }));
    for (const day of data.days) {
      const classes = ['cell', 'day-head'];
      if (day.is_today) classes.push('today');
      if (day.is_weekend) classes.push('weekend');
      grid.appendChild(
        el('div', { class: classes.join(' '), title: day.date }, [
          el('span', { class: 'dow', text: day.weekday }),
          el('span', { class: 'dom', text: String(day.day) }),
        ]),
      );
    }
    grid.appendChild(el('div', { class: 'cell total-head', text: '✓' }));

    for (const habit of data.habits) {
      grid.appendChild(habitName(habit));
      habit.done.forEach((done, index) => {
        grid.appendChild(tickCell(habit, data.days[index], done));
      });
      grid.appendChild(
        el('div', { class: 'cell total', title: `${habit.count} this month` }, [
          el('span', { text: String(habit.count) }),
          habit.streak > 0 ? el('span', { class: 'streak', text: `${habit.streak}d` }) : null,
        ]),
      );
    }

    if (!data.habits.length) {
      grid.appendChild(
        el('div', {
          class: 'cell empty-row',
          style: `grid-column: 1 / span ${data.days.length + 2}`,
          text: 'No habits yet — add one below.',
        }),
      );
    }

    // The two hand-entered numbers sit under the habits as their own rows:
    // same month, same columns, so they read against the ticks.
    numberRow('screen', 'Screen time', data.screen);
    numberRow('sleep', 'Sleep', data.sleep);
  }

  function habitName(habit) {
    const name = el('span', { class: 'habit-label', text: habit.name });
    name.addEventListener('dblclick', () => {
      const input = el('input', { class: 'inline-input', type: 'text', value: habit.name });
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

    const remove = el('button', {
      class: 'icon-button',
      type: 'button',
      title: `Delete “${habit.name}”`,
      text: '✕',
      onclick: async () => {
        await call('delete_habit', { id: habit.id }, 'Delete habit');
        load();
      },
    });

    return el('div', { class: 'cell name' }, [name, remove]);
  }

  function tickCell(habit, day, done) {
    const classes = ['cell', 'tick'];
    if (done) classes.push('done');
    if (day.is_today) classes.push('today');
    if (day.is_weekend) classes.push('weekend');
    // A day that has not happened cannot have been done.
    if (day.is_future) classes.push('future');

    const cell = el('div', {
      class: classes.join(' '),
      title: `${habit.name} · ${day.date}`,
      role: 'button',
      'aria-label': `${habit.name} on ${day.date}`,
      text: done ? '✓' : '',
    });
    if (!day.is_future) {
      cell.addEventListener('click', async () => {
        await call('toggle_habit', { id: habit.id, date: day.date }, 'Habit');
        load();
      });
    }
    return cell;
  }

  function numberRow(field, title, values) {
    grid.appendChild(el('div', { class: 'cell name number-name' }, [
      el('span', { class: 'habit-label', text: title }),
    ]));

    values.forEach((minutes, index) => {
      const day = data.days[index];
      const classes = ['cell', 'number'];
      if (day.is_today) classes.push('today');
      if (day.is_weekend) classes.push('weekend');
      if (day.is_future) classes.push('future');

      // Hours are what the cell has room for; the editor takes any shape.
      const shown = minutes ? (minutes / 60).toFixed(minutes % 60 ? 1 : 0) : '';
      const cell = el('div', {
        class: classes.join(' '),
        title: `${title} · ${day.date}${minutes ? ` · ${Math.floor(minutes / 60)}h ${minutes % 60}m` : ''}`,
        text: shown,
      });
      if (!day.is_future) {
        cell.addEventListener('click', () => editNumber(cell, field, day, minutes));
      }
      grid.appendChild(cell);
    });

    const recorded = values.filter(Boolean);
    const average = recorded.length
      ? Math.round(recorded.reduce((sum, m) => sum + m, 0) / recorded.length)
      : null;
    grid.appendChild(
      el('div', {
        class: 'cell total',
        title: average ? `Average ${Math.floor(average / 60)}h ${average % 60}m` : 'No entries',
        text: average ? `${(average / 60).toFixed(1)}` : '–',
      }),
    );
  }

  function editNumber(cell, field, day, minutes) {
    const input = el('input', {
      class: 'cell-input',
      type: 'text',
      title: DURATION_HINT,
      value: minutes ? `${Math.floor(minutes / 60)}h${minutes % 60 ? minutes % 60 : ''}` : '',
    });
    cell.replaceChildren(input);
    input.focus();
    input.select();

    const commit = async () => {
      await call('set_day_metric', { date: day.date, field, value: input.value }, 'Saving');
      load();
    };
    input.addEventListener('blur', commit);
    input.addEventListener('keydown', (event) => {
      if (event.key === 'Enter') commit();
      if (event.key === 'Escape') load();
    });
  }

  // ---------- the journal ----------

  async function renderJournal() {
    const [year, month] = shown;
    const journal = await call('month_journal', { year, month }, 'Journal');
    clear(entriesNode);
    if (!journal) return;

    if (!journal.entries.length) {
      entriesNode.appendChild(
        el('p', {
          class: 'empty-row',
          text: 'Nothing written this month. Add an entry to start.',
        }),
      );
      return;
    }
    for (const entry of journal.entries) {
      entriesNode.appendChild(entryBlock(year, month, entry));
    }
  }

  function entryBlock(year, month, entry) {
    const save = () =>
      call(
        'save_journal_entry',
        { year, month, entry: { id: entry.id, title: title.value, body: body.value } },
        'Journal',
      );

    const title = el('input', {
      class: 'entry-title',
      type: 'text',
      value: entry.title,
      placeholder: 'Heading — a day, a week, anything',
    });
    title.addEventListener('change', save);

    const body = el('textarea', {
      class: 'entry-body',
      placeholder: 'Write whatever.',
    });
    body.value = entry.body;
    const autosize = () => {
      body.style.height = 'auto';
      body.style.height = `${Math.max(64, body.scrollHeight)}px`;
    };
    body.addEventListener('input', autosize);

    let timer = null;
    body.addEventListener('input', () => {
      clearTimeout(timer);
      timer = setTimeout(save, 600);
    });
    body.addEventListener('blur', save);

    const remove = el('button', {
      class: 'icon-button',
      type: 'button',
      title: 'Delete entry',
      text: '✕',
      onclick: async () => {
        await call('delete_journal_entry', { year, month, id: entry.id }, 'Journal');
        renderJournal();
      },
    });

    const block = el('article', { class: 'entry' }, [
      el('div', { class: 'entry-head' }, [title, remove]),
      body,
    ]);
    // Sizing needs the node in the document, so do it on the next frame.
    requestAnimationFrame(autosize);
    return block;
  }

  async function newEntry() {
    const [year, month] = shown;
    // A new entry is headed with today when today is in the month on screen,
    // which is the common case and saves a keystroke.
    const today = getToday();
    const inMonth = today.getFullYear() === year && today.getMonth() + 1 === month;
    const heading = inMonth
      ? today.toLocaleDateString(undefined, { weekday: 'long', day: 'numeric' })
      : '';

    await call(
      'save_journal_entry',
      { year, month, entry: { id: null, title: heading, body: '' } },
      'Journal',
    );
    await renderJournal();
    entriesNode.querySelector('.entry:last-child .entry-body')?.focus();
  }

  newHabit.addEventListener('keydown', async (event) => {
    if (event.key !== 'Enter' || !newHabit.value.trim()) return;
    await call('add_habit', { name: newHabit.value.trim() }, 'Add habit');
    newHabit.value = '';
    load();
  });
  addEntry.addEventListener('click', newEntry);

  return { load, step, thisMonth };
}

/** The ISO date of "today", for callers that need it without a round trip. */
export const todayIso = () => toIso(new Date());
