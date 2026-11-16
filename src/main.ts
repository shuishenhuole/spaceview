import { createApp } from "vue";
import App from "./App.vue";
import { router } from "./router";

import "./style.css";
import "./styles/theme.less";
import "./styles/app.less";

createApp(App).use(router).mount("#app");
