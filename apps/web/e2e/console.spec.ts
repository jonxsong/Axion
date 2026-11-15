import { execFileSync } from "node:child_process";
import path from "node:path";

import { expect, test, type APIRequestContext, type Page } from "@playwright/test";

const collection = "e2e_browser";
const repository = path.resolve(__dirname, "../../..");
const assets = path.join(repository, "docs", "assets");

async function waitForReady(request: APIRequestContext) {
  await expect
    .poll(async () => (await request.get("/api/health/ready")).status(), { timeout: 60_000 })
    .toBe(200);
}

async function openSection(page: Page, name: string) {
  await page.getByRole("button", { name, exact: true }).click();
  await expect(page.getByRole("heading", { name, exact: true }).last()).toBeVisible();
}

test("real console flow survives an Axion restart", async ({ page, request }) => {
  await waitForReady(request);
  await request.delete(`/api/v1/collections/${collection}`);

  await page.goto("/console");
  await expect(page.getByText("Engine ready")).toBeVisible();
  await expect(page.getByRole("heading", { name: "System overview" })).toBeVisible();
  await page.screenshot({ path: path.join(assets, "console-overview.png"), fullPage: true });

  await openSection(page, "Collections");
  await page.getByRole("button", { name: "New collection" }).click();
  await page.getByLabel("Name").fill(collection);
  await page.getByLabel("Dimensions").fill("3");
  await page.getByLabel("Metric").selectOption("cosine");
  await page.getByRole("button", { name: "Create durable collection" }).click();
  await expect(page.getByRole("cell", { name: collection, exact: true })).toBeVisible();
  await page.screenshot({ path: path.join(assets, "console-collections.png"), fullPage: true });

  const inserted = await request.post(`/api/v1/collections/${collection}/vectors/batch`, {
    data: {
      vectors: [
        { id: "research-1", values: [1, 0, 0], metadata: { category: "research" } },
        { id: "engineering-1", values: [0.9, 0.1, 0], metadata: { category: "engineering" } },
        { id: "other-1", values: [0, 1, 0], metadata: { category: "other" } },
      ],
    },
  });
  expect(inserted.ok()).toBeTruthy();

  await openSection(page, "Vector Explorer");
  await page.getByLabel("Collection", { exact: true }).selectOption(collection);
  await expect(page.getByRole("cell", { name: "research-1", exact: true })).toBeVisible();
  await expect(page.getByLabel("Projected vector research-1")).toBeVisible();
  await page.screenshot({ path: path.join(assets, "console-vector-explorer.png"), fullPage: true });

  await openSection(page, "Search Playground");
  await page.getByLabel("Collection", { exact: true }).selectOption(collection);
  await page.getByLabel("Query vector").fill("[1, 0, 0]");
  await page.getByLabel("Top K").fill("2");
  await page.getByRole("button", { name: "Run search" }).click();
  await expect(page.getByRole("cell", { name: "research-1", exact: true })).toBeVisible();
  await page.screenshot({ path: path.join(assets, "console-search-playground.png"), fullPage: true });

  execFileSync("docker", ["compose", "-p", process.env.AXION_E2E_PROJECT ?? "axion-e2e", "restart", "axion"], {
    cwd: repository,
    env: process.env,
    stdio: "inherit",
  });
  await waitForReady(request);
  await page.reload();
  await openSection(page, "Search Playground");
  await page.getByLabel("Collection", { exact: true }).selectOption(collection);
  await page.getByLabel("Query vector").fill("[1, 0, 0]");
  await page.getByLabel("Top K").fill("2");
  await page.getByRole("button", { name: "Run search" }).click();
  await expect(page.getByRole("cell", { name: "research-1", exact: true })).toBeVisible();

  await page.getByLabel("Query vector").fill("[1]");
  await page.getByRole("button", { name: "Run search" }).click();
  await expect(page.getByText(/dimension mismatch/i)).toBeVisible();

  await openSection(page, "Collections");
  page.once("dialog", (dialog) => dialog.accept());
  await page.getByRole("button", { name: `Delete collection ${collection}` }).click();
  await expect(page.getByRole("cell", { name: collection, exact: true })).toHaveCount(0);
});

test("console remains usable at a mobile viewport", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/console");
  await expect(page.getByRole("button", { name: "Collections" })).toBeVisible();
  await page.getByRole("button", { name: "Collections" }).click();
  await expect(page.getByRole("heading", { name: "Collections", exact: true }).last()).toBeVisible();
  await expect(page.locator("body")).not.toHaveCSS("overflow-x", "scroll");
});
