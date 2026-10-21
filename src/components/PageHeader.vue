<script setup lang="ts">

import { useI18n } from '@/i18n';
import type { Section } from '@/types/sync';

import refreshIcon from '@/assets/icons/refresh.svg?url';

interface Props {
  section: Section;
  busy: boolean;
}

defineProps<Props>();

const emit = defineEmits<{ refresh: [] }>();

const { t } = useI18n();

</script>

<template>
  <header class="page_header">
    <div class="page_heading">
      <h1>{{ section === 'syncs' ? t.syncs : t.devices }}</h1>
      <button
        type="button"
        class="app_button app_button--icon"
        :disabled="busy"
        :aria-label="t.refresh"
        :title="t.refresh"
        data-testid="refresh"
        @click="emit('refresh')"
      >
        <img class="app_icon" :src="refreshIcon" alt="" aria-hidden="true" />
      </button>
    </div>
    <p>{{ section === 'syncs' ? t.intro : t.devicesIntro }}</p>
  </header>
</template>

<style scoped lang="scss">

.page_header {
  margin-bottom: $sync_space_large;
}

.page_heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: $sync_space;
}

</style>
