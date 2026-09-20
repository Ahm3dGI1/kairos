// The settings page.
//
// The switches are not listed here. The core describes them — key, section,
// label, why you would want it, and whether changing it needs a restart — and
// this file renders whatever it is given. Adding a switch is a change to one
// Rust array; this page picks it up with no edit at all.

import { call, clear, el } from './shared.js';

export function createSettings({ container, onChange }) {
  let settings = [];
  let vault = null;

  async function load() {
    settings = (await call('settings', undefined, 'Reading settings')) ?? [];
    vault = await call('vault_info', undefined, 'Reading the vault');
    render();
  }

  async function set(key, value) {
    const updated = await call('set_setting', { key, value }, 'Changing a setting');
    if (!updated) return;
    settings = updated;
    vault = await call('vault_info', undefined, 'Reading the vault');
    render();
    onChange?.();
  }

  function control(setting) {
    if (setting.kind === 'choice') {
      const select = el('select', { class: 'setting-choice' });
      for (const [value, label] of setting.options) {
        select.appendChild(
          el('option', { value, text: label, selected: value === setting.value ? '' : null }),
        );
      }
      select.addEventListener('change', () => set(setting.key, select.value));
      return select;
    }

    if (setting.kind === 'path') {
      const input = el('input', {
        class: 'setting-path',
        type: 'text',
        value: setting.value ?? '',
        placeholder: setting.placeholder,
        spellcheck: 'false',
      });
      // On blur rather than on every keystroke: a path is only meaningful once
      // it is finished being typed.
      input.addEventListener('blur', () => {
        if (input.value !== setting.value) set(setting.key, input.value);
      });
      input.addEventListener('keydown', (event) => {
        if (event.key === 'Enter') input.blur();
      });
      return input;
    }

    const button = el('button', {
      class: `switch ${setting.value ? 'on' : ''}`,
      type: 'button',
      role: 'switch',
      'aria-checked': setting.value ? 'true' : 'false',
      'aria-label': setting.label,
    }, [el('span', { class: 'knob' })]);
    button.addEventListener('click', () => set(setting.key, !setting.value));
    return button;
  }

  function row(setting) {
    return el('div', { class: 'setting' }, [
      el('div', { class: 'setting-text' }, [
        el('div', { class: 'setting-label' }, [
          setting.label,
          setting.restart ? el('span', { class: 'restart', text: 'restart' }) : null,
        ]),
        el('div', { class: 'setting-detail', text: setting.detail }),
      ]),
      control(setting),
    ]);
  }

  /** The Storage section earns a few extra controls the schema cannot describe. */
  function vaultTools() {
    if (!vault) return null;

    const summary = vault.enabled
      ? `${vault.files} file${vault.files === 1 ? '' : 's'}`
      : 'off — the database is the only copy';

    return el('div', { class: 'vault-tools' }, [
      el('div', { class: 'vault-state' }, [
        el('code', { class: 'vault-path', text: vault.path }),
        el('span', { class: 'vault-count', text: summary }),
      ]),
      el('div', { class: 'vault-buttons' }, [
        el('button', {
          class: 'ghost',
          type: 'button',
          text: 'Open folder',
          onclick: () => call('open_vault', undefined, 'Opening the vault'),
        }),
        el('button', {
          class: 'ghost',
          type: 'button',
          text: 'Reload from files',
          title: 'Rebuild the database from the files now, without waiting for the watcher',
          onclick: async () => {
            await call('reload_vault', undefined, 'Reloading the vault');
            onChange?.();
            load();
          },
        }),
        el('button', {
          class: 'ghost',
          type: 'button',
          text: 'Rewrite files',
          title: 'Write every file out again from the database',
          onclick: async () => {
            await call('rewrite_vault', undefined, 'Rewriting the vault');
            load();
          },
        }),
      ]),
      el('p', { class: 'vault-note' }, [
        'These files are the real data — the database is an index rebuilt from them. ',
        'Edit them in any editor and the app catches up within a couple of seconds. ',
        el('code', { text: 'README.md' }),
        ' in the folder explains the formats, and your switches are in ',
        el('code', { text: vault.settings_file }),
        '.',
      ]),
    ]);
  }

  function render() {
    clear(container);

    const sections = [];
    for (const setting of settings) {
      let section = sections.find((s) => s.name === setting.section);
      if (!section) sections.push((section = { name: setting.section, rows: [] }));
      section.rows.push(setting);
    }

    for (const section of sections) {
      container.appendChild(
        el('div', { class: 'section-rule' }, [
          el('span', { class: 'rule' }),
          el('span', { text: section.name.toUpperCase() }),
          el('span', { class: 'rule' }),
        ]),
      );
      const body = el('div', { class: 'settings-group' }, section.rows.map(row));
      if (section.name === 'Storage') {
        const tools = vaultTools();
        if (tools) body.appendChild(tools);
      }
      container.appendChild(body);
    }
  }

  return { load, render };
}
