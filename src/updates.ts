import {invoke} from '@tauri-apps/api/core';
import {listen} from '@tauri-apps/api/event';

export async function checkForUpdate(hasEdits: () => boolean, notify: (text: string) => void) {
  if (hasEdits()) {
    notify('Save or discard your edits before checking for updates.');
    return;
  }
  const button = document.querySelector<HTMLButtonElement>('#update')!;
  if (button.disabled) return;
  const dialog = document.createElement('dialog');
  const heading = document.createElement('h2');
  heading.id = 'update-heading';
  heading.textContent = 'Check for Updates';
  dialog.setAttribute('aria-labelledby', heading.id);
  const status = document.createElement('p');
  status.setAttribute('role', 'status');
  status.textContent = 'Checking GitHub Releases…';
  dialog.append(heading, status);
  document.body.append(dialog);
  button.disabled = true;
  dialog.oncancel = event => event.preventDefault();
  dialog.showModal();
  let unsubscribe: (() => void) | undefined;
  try {
    unsubscribe = await listen<string>('update-status', event => { status.textContent = event.payload; });
    status.textContent = await invoke<string>('check_for_update');
  } catch (error) {
    status.textContent = String(error);
  } finally {
    unsubscribe?.();
    dialog.oncancel = null;
    const close = document.createElement('button');
    close.textContent = 'Close';
    close.className = 'primary';
    close.onclick = () => dialog.close();
    dialog.onclose = () => { dialog.remove(); button.disabled = false; button.focus(); };
    dialog.append(close);
    close.focus();
  }
}

