import { guardFrame } from '@valeriogc/nas-base-app';
import { createApp } from 'vue';
import App from '@/App.vue';
import '@/styles/main.scss';

// In the frame of the launcher, the rules its sandbox used to apply, before anything else runs.
guardFrame();

createApp(App).mount('#app');
