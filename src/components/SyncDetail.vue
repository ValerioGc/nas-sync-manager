<script setup lang="ts">

import { useI18n } from '@/i18n';
import { formatBytes } from '@valeriogc/nas-base-app';
import { diskPath, formatDate } from '@/services/sync';
import type { SyncDetail, Volume } from '@/types/sync';

interface Props {
  detail: SyncDetail;
  volumes: Volume[];
  busy: boolean;
}

defineProps<Props>();

const emit = defineEmits<{
  close: [];
  pause: [];
  revoke: [];
  resolve: [path: string, keep: 'this' | 'other'];
}>();

const { t, locale } = useI18n();

function action(value: string) {
  return t.value.changeActions[value as keyof typeof t.value.changeActions] ?? value;
}

</script>

<template>
  <section data-testid="sync_detail">
    <div class="app_toolbar">
      <h1>{{ detail.sync.name }}</h1>
      <button class="app_button" data-testid="detail_close" @click="emit('close')">
        {{ t.close }}
      </button>
      <button
        class="app_button"
        :disabled="busy || detail.sync.revoked"
        data-testid="detail_pause"
        @click="emit('pause')"
      >
        {{ detail.sync.paused ? t.resume : t.pause }}
      </button>
      <button
        class="app_button"
        :disabled="busy || detail.sync.revoked"
        data-testid="detail_revoke"
        @click="emit('revoke')"
      >
        {{ t.revoke }}
      </button>
    </div>

    <!-- Detail metrics -->
    <dl class="detail_metrics">
      <dt>{{ t.nasFolder }}</dt>
      <dd>{{ diskPath(detail.sync.nasPath, volumes) }}</dd>
      <dt>{{ t.computer }}</dt>
      <dd>{{ detail.sync.deviceName }}</dd>
      <dt>{{ t.pcFolder }}</dt>
      <dd>{{ detail.sync.pcPath }}</dd>
      <dt>{{ t.mode }}</dt>
      <dd>{{ t.modes[detail.sync.mode] }}</dd>
      <dt>{{ t.state }}</dt>
      <dd>{{ t.states[detail.sync.state] }}</dd>
      <dt>{{ t.files }}</dt>
      <dd>{{ detail.sync.globalFiles }}</dd>
      <dt>{{ t.space }}</dt>
      <dd>{{ formatBytes(detail.sync.globalBytes, locale) }}</dd>
      <dt>{{ t.remaining }}</dt>
      <dd>{{ detail.sync.needFiles }} / {{ formatBytes(detail.sync.needBytes, locale) }}</dd>
      <dt>{{ t.lastSync }}</dt>
      <dd>{{ formatDate(detail.sync.lastSync, locale) }}</dd>
    </dl>
    <progress
      v-if="detail.sync.progress !== null"
      :value="detail.sync.progress"
      max="100"
      :aria-label="t.progress"
    ></progress>

    <!-- Detail errors -->
    <p v-if="detail.sync.error" class="app_error">{{ detail.sync.error }}</p>
    <p v-if="detail.sync.trashOff" class="trash_off" role="note" data-testid="detail_trash_off">
      {{ t.trashOff }}
    </p>
    <h2>{{ t.conflicts }}</h2>
    <p v-if="!detail.conflicts.length">{{ t.noConflicts }}</p>
    <article
      v-for="conflict in detail.conflicts"
      :key="conflict.path"
      class="app_panel conflict_row"
    >
      <p>{{ t.original }}: {{ conflict.original }}</p>
      <p>{{ t.conflictCopy }}: {{ conflict.path }} ({{ formatBytes(conflict.size, locale) }})</p>
      <div class="app_toolbar">
        <button
          class="app_button"
          :disabled="busy || detail.sync.revoked"
          :data-testid="`keep_this_${conflict.path}`"
          @click="emit('resolve', conflict.path, 'this')"
        >
          {{ t.keepThis }}
        </button>
        <button
          class="app_button"
          :disabled="busy || detail.sync.revoked"
          :data-testid="`keep_other_${conflict.path}`"
          @click="emit('resolve', conflict.path, 'other')"
        >
          {{ t.keepOther }}
        </button>
      </div>
    </article>

    <!-- Detail conflicts -->
    <h2>{{ t.changes }}</h2>
    <p v-if="!detail.changes.length">{{ t.noChanges }}</p>
    <ul v-else>
      <li v-for="(change, index) in detail.changes" :key="index">
        {{ formatDate(change.time, locale) }}: {{ action(change.action) }} {{ change.path }}
      </li>
    </ul>
    <h2>{{ t.ignores }}</h2>
    <p v-if="!detail.ignores.length">{{ t.noIgnores }}</p>
    <ul v-else>
      <li v-for="ignore in detail.ignores" :key="ignore">
        <code>{{ ignore }}</code>
      </li>
    </ul>
  </section>
</template>

<style scoped lang="scss">

.detail_metrics {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  gap: $sync_space;
}

dd {
  margin: 0;
  overflow-wrap: anywhere;
}

.trash_off {
  padding: $sync_space;
  border-left: 3px solid var(--tone-warning);
  border-radius: 6px;
  background: color-mix(in srgb, var(--tone-warning) 10%, transparent);
}

.conflict_row {
  padding: $sync_space;
  margin-bottom: $sync_space;
  overflow-wrap: anywhere;
}

</style>
