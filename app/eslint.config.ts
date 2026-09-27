import { factoryConfig } from "@cany748/eslint-config";

export default factoryConfig({
  typescript: { tsconfigPath: "./tsconfig.json", filesTypeAware: ["**/*.{ts,vue}"] },
});
