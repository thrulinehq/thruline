const { test, expect } = require('@playwright/test');
test('reject invalid amounts; save valid amount and retain it after reload', async ({page})=>{
  await page.goto('/');
  await page.getByRole('spinbutton',{name:'Amount'}).fill('-4');
  await page.getByRole('button',{name:'Save'}).click();
  await expect(page.getByRole('alert')).toHaveText('Enter a positive amount');
  await expect(page.locator('ul')).toHaveText('');
  await page.getByRole('spinbutton',{name:'Amount'}).fill('12');
  await page.getByRole('button',{name:'Save'}).click();
  await expect(page.locator('ul')).toHaveText('12');
  await page.reload();
  await expect(page.locator('ul')).toHaveText('12');
});
