import { test, expect } from '@playwright/test';
const pc = 'PCPCPCP-AAAAAAA-BBBBBBB-CCCCCCC-DDDDDDD-EEEEEEE-FFFFFFF-GGGGGGG';
test('manages a real backend sync, resolves a conflict, and retains NAS files on revocation', async ({
  page,
  request,
}, testInfo) => {
  await expect
    .poll(
      async () =>
        ((await (await request.get('/api/status')).json()) as { running: boolean }).running,
    )
    .toBe(true);
  const pairing = await request.post('/api/client/pair', {
    headers: { 'x-nas-client': pc },
    data: { deviceId: pc, name: 'Portatile', version: '0.1.0' },
  });
  expect(pairing.ok()).toBe(true);
  const created = await request.post('/api/client/syncs', {
    headers: { 'x-nas-client': pc },
    data: { name: 'Foto', nasPath: 'Foto', pcPath: '/home/Foto', mode: 'two-way' },
  });
  expect(created.ok()).toBe(true);
  const { id } = (await created.json()) as { id: string };
  await page.goto('/');
  await expect(page.getByTestId(`sync_${id}`)).toContainText('Sincronizzato');
  await page.screenshot({ path: testInfo.outputPath('sync-list.png'), fullPage: true });
  await page.getByTestId(`pause_${id}`).click();
  await expect(page.getByTestId(`sync_${id}`)).toContainText('In pausa');
  await page.getByTestId(`pause_${id}`).click();
  await expect(page.getByTestId(`sync_${id}`)).toContainText('Sincronizzato');
  await page.getByTestId(`detail_${id}`).click();
  await expect(page.getByTestId('sync_detail')).toContainText('*.tmp');
  await expect(page.getByTestId('sync_detail')).toContainText('Modificato a.txt');
  await page.getByTestId('keep_other_a.sync-conflict-20261010-120000-ABCDEFG.txt').click();
  await expect(page.getByRole('dialog')).toContainText('cestino');
  await page.getByTestId('confirm_action').click();
  await expect(page.getByTestId('sync_detail')).toContainText('Nessun conflitto');
  await page.getByTestId('detail_close').click();
  await page.getByTestId(`revoke_${id}`).click();
  await page.getByTestId('confirm_cancel').click();
  await expect(page.getByTestId(`sync_${id}`)).toContainText('Sincronizzato');
  await page.getByTestId(`revoke_${id}`).click();
  await page.getByTestId('confirm_action').click();
  await expect(page.getByTestId(`sync_${id}`)).toContainText('Revocata');
  await expect(page.getByTestId(`pause_${id}`)).toBeDisabled();
  const second = await request.post('/api/client/syncs', {
    headers: { 'x-nas-client': pc },
    data: { name: 'Foto nuove', nasPath: 'Foto', pcPath: '/home/Foto', mode: 'pc-to-nas' },
  });
  expect(second.ok()).toBe(true);
  await page.getByTestId('nav_devices').click();
  await expect(page.getByRole('table')).toContainText('Portatile');
  await page.getByTestId(`device_revoke_${pc}`).click();
  await page.getByTestId('confirm_action').click();
  await expect(page.getByTestId('device_empty')).toBeVisible();
  expect(
    (await request.get('/api/client/syncs', { headers: { 'x-nas-client': pc } })).status(),
  ).toBe(403);
  await page.getByTestId('nav_syncs').click();
  await expect(page.getByTestId('sync_empty')).toBeVisible();
});
test('saves bandwidth and scan settings across reload and adapts language and theme', async ({
  page,
  request,
}) => {
  await page.goto('/?lang=en&theme=dark');
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await expect(page.getByTestId('nav_syncs')).toHaveText('Synchronizations');
  await page.getByTestId('nav_settings').click();
  await page.getByTestId('settings_send').fill('100');
  await page.getByTestId('settings_receive').fill('200');
  // A number out of its bounds is not taken: the field goes back to the value it had.
  const rescan = page.getByTestId('settings_rescan');
  const before = await rescan.inputValue();
  await rescan.fill('0');
  await rescan.blur();
  await expect(rescan).toHaveValue(before);
  await rescan.fill('15');
  await page.getByTestId('settings_save').click();
  await expect(page.getByTestId('settings_send')).toHaveCount(0);
  expect(await (await request.get('/api/settings')).json()).toEqual({
    maxSendKbps: 100,
    maxRecvKbps: 200,
    rescanMinutes: 15,
  });
  await page.reload();
  await page.getByTestId('nav_settings').click();
  await expect(page.getByTestId('settings_send')).toHaveValue('100');
  await expect(page.getByTestId('settings_rescan')).toHaveValue('15');
  await page.getByTestId('settings_close').click();
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(page.getByTestId('nav_devices')).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(
    true,
  );
});
