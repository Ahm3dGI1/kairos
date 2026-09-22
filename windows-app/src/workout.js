// The workout book.
//
// The spreadsheet this replaces had one page per training day, the exercises
// down the left and a pair of columns per session to the right. That shape is
// kept exactly, because it is the shape that lets you see the only thing worth
// seeing: what you did last time, next to what you are doing now.
//
// Sessions run newest-first from the left, so today sits where you look first.

import { call, clear, el } from './shared.js';

export function createWorkout({ routinesNode, gridNode, emptyNode, onRoutine }) {
  let page = null;
  let routineId = null;

  async function load() {
    page = await call('workout_page', { routine: routineId }, 'Workout');
    if (!page) return;
    routineId = page.routine;

    onRoutine?.(page.name || 'Workout', page.sessions.length);
    renderRoutines();
    renderGrid();
  }

  /**
   * Edits a session's note, in a field floated over its column.
   *
   * The column is two cells wide and holds a date, which leaves no room for
   * the note itself — so the button says whether there is one and opens it.
   */
  function editNote(session, anchor) {
    gridNode.querySelector('.note-input')?.remove();
    const input = el('input', {
      class: 'note-input',
      type: 'text',
      value: session.note ?? '',
      placeholder: 'how it went',
      spellcheck: 'false',
    });
    input.style.left = `${anchor.offsetLeft - 80}px`;
    input.style.top = `${anchor.offsetTop + 22}px`;
    gridNode.appendChild(input);
    input.focus();
    input.select();

    let closed = false;
    const commit = async (save) => {
      if (closed) return;
      closed = true;
      const note = input.value;
      input.remove();
      if (!save || note === (session.note ?? '')) return;
      await call('save_session_note', { session: session.id, note }, 'Session note');
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

  function select(id) {
    routineId = id;
    load();
  }

  // ---------- routines: the pages of the book ----------

  function renderRoutines() {
    clear(routinesNode);
    routinesNode.appendChild(el('div', { class: 'lists-head', text: 'ROUTINES' }));

    for (const routine of page.routines) {
      const row = el('button', {
        type: 'button',
        class: `list-row ${routine.id === routineId ? 'on' : ''}`.trim(),
        onclick: () => select(routine.id),
      }, [
        el('span', { class: 'name', text: routine.name }),
        el('span', { class: 'n', text: String(routine.sessions) }),
        el('button', {
          type: 'button',
          class: 'x',
          text: '✕',
          title: 'Delete this routine and every session of it',
          onclick: async (event) => {
            event.stopPropagation();
            const count = routine.sessions;
            const warning = count
              ? `Delete "${routine.name}" and its ${count} session${count === 1 ? '' : 's'}?`
              : `Delete "${routine.name}"?`;
            if (!window.confirm(warning)) return;
            await call('delete_routine', { id: routine.id }, 'Delete routine');
            if (routine.id === routineId) routineId = null;
            load();
          },
        }),
      ]);

      // Double-click renames, matching the habit rows.
      row.addEventListener('dblclick', () => {
        const input = el('input', { class: 'inline', type: 'text', value: routine.name });
        row.replaceWith(input);
        input.focus();
        input.select();
        const commit = async () => {
          await call('rename_routine', { id: routine.id, name: input.value }, 'Rename');
          load();
        };
        input.addEventListener('blur', commit);
        input.addEventListener('keydown', (event) => {
          if (event.key === 'Enter') commit();
          if (event.key === 'Escape') load();
        });
      });
      routinesNode.appendChild(row);
    }

    const adder = el('input', {
      class: 'inline',
      type: 'text',
      placeholder: '+ routine',
      'aria-label': 'Add a routine',
    });
    adder.addEventListener('keydown', async (event) => {
      if (event.key !== 'Enter' || !adder.value.trim()) return;
      await call('add_routine', { name: adder.value.trim() }, 'Add routine');
      adder.value = '';
      load();
    });
    routinesNode.appendChild(adder);
  }

  // ---------- the grid ----------

  function renderGrid() {
    clear(gridNode);
    if (!page.routine) {
      emptyNode.hidden = false;
      emptyNode.textContent = 'no routines yet — add one on the left';
      return;
    }
    emptyNode.hidden = true;

    const sessions = page.sessions;
    // Name column, set number, then reps and weight per session.
    gridNode.style.gridTemplateColumns = `156px 22px repeat(${sessions.length}, 40px 46px)`;

    // Header: the dates, each spanning its two columns.
    gridNode.append(
      el('div', { class: 'wcell head sticky-col' }),
      el('div', { class: 'wcell head' }),
    );
    for (const session of sessions) {
      gridNode.appendChild(
        el('div', {
          class: `wcell head date ${session.is_today ? 'today' : ''}`.trim(),
          style: 'grid-column: span 2',
          title: session.note || session.label,
        }, [
          el('span', { text: session.is_today ? 'today' : session.label }),
          el('button', {
            type: 'button',
            class: `note ${session.note ? 'on' : ''}`.trim(),
            title: session.note || 'Add a note to this session',
            text: '·',
            onclick: (event) => editNote(session, event.currentTarget),
          }),
          el('button', {
            type: 'button',
            class: 'x',
            title: 'Delete this session',
            text: '✕',
            onclick: async () => {
              await call('delete_session', { session: session.id }, 'Delete session');
              load();
            },
          }),
        ]),
      );
    }

    // Sub-header: what the two columns under each date mean.
    gridNode.append(
      el('div', { class: 'wcell sub sticky-col' }),
      el('div', { class: 'wcell sub', text: '#' }),
    );
    for (const _ of sessions) {
      gridNode.append(
        el('div', { class: 'wcell sub', text: 'reps' }),
        el('div', { class: 'wcell sub', text: 'kg' }),
      );
    }

    for (const exercise of page.exercises) {
      renderExercise(exercise, sessions);
    }

    // A row to add the next exercise, in the name column.
    const adder = el('input', { class: 'inline', type: 'text', placeholder: '+ exercise' });
    adder.addEventListener('keydown', async (event) => {
      if (event.key !== 'Enter' || !adder.value.trim()) return;
      await call(
        'add_exercise',
        { routine: page.routine, name: adder.value.trim() },
        'Add exercise',
      );
      adder.value = '';
      load();
    });
    gridNode.appendChild(
      el('div', { class: 'wcell name sticky-col adder', style: 'grid-column: 1 / -1' }, [adder]),
    );
  }

  function renderExercise(exercise, sessions) {
    const rows = page.max_sets;

    for (let index = 1; index <= rows; index += 1) {
      // The name sits on the first row of its block and spans the rest.
      if (index === 1) {
        const name = el('span', { class: 'label', text: exercise.name });
        name.addEventListener('dblclick', () => {
          const input = el('input', { class: 'inline', type: 'text', value: exercise.name });
          name.replaceWith(input);
          input.focus();
          input.select();
          const commit = async () => {
            await call(
              'rename_exercise',
              { routine: page.routine, id: exercise.id, name: input.value },
              'Rename',
            );
            load();
          };
          input.addEventListener('blur', commit);
          input.addEventListener('keydown', (event) => {
            if (event.key === 'Enter') commit();
            if (event.key === 'Escape') load();
          });
        });

        gridNode.appendChild(
          el('div', {
            class: 'wcell name sticky-col block-start',
            style: `grid-row: span ${rows}`,
          }, [
            name,
            el('button', {
              type: 'button',
              class: 'x',
              title: `Delete ${exercise.name}`,
              text: '✕',
              onclick: async () => {
                await call('delete_exercise', { id: exercise.id }, 'Delete exercise');
                load();
              },
            }),
          ]),
        );
      }

      gridNode.appendChild(
        el('div', {
          class: `wcell num ${index === 1 ? 'block-start' : ''}`.trim(),
          text: String(index),
        }),
      );

      for (const session of sessions) {
        const set = session.sets.find(
          (s) => s.exercise === exercise.id && s.index === index,
        );
        gridNode.append(
          field(session, exercise, index, 'reps', set ? String(set.reps) : '', index === 1),
          field(session, exercise, index, 'weight', set ? set.weight_label : '', index === 1),
        );
      }
    }
  }

  /** One editable cell. Blur or Enter writes; an emptied pair deletes the set. */
  function field(session, exercise, index, kind, value, blockStart) {
    const input = el('input', {
      class: `wcell cell ${kind} ${blockStart ? 'block-start' : ''}`.trim(),
      type: 'text',
      value,
      inputmode: kind === 'reps' ? 'numeric' : 'decimal',
      'aria-label': `${exercise.name}, set ${index}, ${kind}`,
    });

    const save = async () => {
      const row = input.closest('.workout-grid');
      // Read the pair together so one blank does not wipe the other.
      const reps = kind === 'reps' ? input.value : siblingValue(input, 'reps');
      const weight = kind === 'weight' ? input.value : siblingValue(input, 'weight');
      await call(
        'save_set',
        {
          edit: {
            session: session.id,
            exercise: exercise.id,
            index,
            reps: Number.parseInt(reps, 10) || 0,
            weight,
          },
        },
        'Saving set',
      );
      if (row) load();
    };

    input.addEventListener('change', save);
    input.addEventListener('keydown', (event) => {
      if (event.key === 'Enter') input.blur();
    });
    return input;
  }

  /** The other half of a reps/weight pair, which sits next to this one. */
  function siblingValue(input, kind) {
    const sibling = kind === 'reps' ? input.previousElementSibling : input.nextElementSibling;
    return sibling?.classList.contains(kind) ? sibling.value : '';
  }

  /** Starts today's session, carrying last time's numbers across. */
  async function startSession() {
    if (!page?.routine) return false;
    await call('start_session', { routine: page.routine }, 'Start session');
    await load();
    return true;
  }

  return { load, select, startSession, routine: () => page?.routine ?? null };
}
