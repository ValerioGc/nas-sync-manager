import { guardFrame } from '@valeriogc/nas-base-app';
import { createApp } from 'vue';
import App from '@/App.vue';
import '@/styles/main.scss';

guardFrame();

createApp(App).mount('#app');
