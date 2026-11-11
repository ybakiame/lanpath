// The Rust root tested from TypeScript: `nsh test` runs this file in a
// headless runner it builds with the crate, and `open(".")` starts the root
// (`nsh.json` "root").
import { expect, open, test } from "nsh/test";

test("parses a subscription through the shared Rust client", async () => {
  const app = await open(".");
  expect(app.texts()).toContain("尚未解析共享订阅。");
  await app.press("解析共享订阅");
  expect(app.texts()).toContain("解析成功：2 个节点\n格式：ClashYaml\n警告：0 条\n\n东京测试节点\n新加坡测试节点");
});
