<script setup lang="ts">

import { NasEmptyState } from '@valeriogc/nas-base-app';
import { useI18n } from '@/i18n';
import { diskPath, formatDate } from '@/services/sync';
import type { ListedSync, Volume } from '@/types/sync';
import syncIcon from '@/assets/icons/sync.svg?url';
interface Props {
  syncs: ListedSync[];
  volumes: Volume[];
  busy: boolean;
}
defineProps<Props>();
const emit = defineEmits<{
  detail: [id: string];
  pause: [sync: ListedSync];
  revoke: [id: string];
}>();
const { t, locale } = useI18n();

</script>

<template>
  <NasEmptyState
    v-if="!syncs.length"
    class="nas_empty_framed"
    :title="t.emptySyncs"
    :description="t.emptySyncsNote"
    data-testid="sync_empty"
  >
    <template #icon>
      <span class="nas_empty_badge" aria-hidden="true">
        <img class="app_icon" :src="syncIcon" alt="" />
      </span>
    </template>
  </NasEmptyState>
  <div v-else class="app_scroll">
    <table>
      <thead>
        <tr>
          <th>{{ t.name }}</th>
          <th>{{ t.nasFolder }}</th>
          <th>{{ t.computer }}</th>
          <th>{{ t.pcFolder }}</th>
          <th>{{ t.mode }}</th>
          <th>{{ t.state }}</th>
          <th>{{ t.lastSync }}</th>
          <th>{{ t.actions }}</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="sync in syncs" :key="sync.id" :data-testid="`sync_${sync.id}`">
          <td>{{ sync.name }}</td>
          <td>{{ diskPath(sync.nasPath, volumes) }}</td>
          <td>{{ sync.deviceName }}</td>
          <td>{{ sync.pcPath }}</td>
          <td>{{ t.modes[sync.mode] }}</td>
          <td>
            {{ t.states[sync.state]
            }}<progress
              v-if="sync.progress !== null"
              :value="sync.progress"
              max="100"
              :aria-label="t.progress"
            /><span v-if="sync.error" class="app_error">{{ sync.error }}</span>
          </td>
          <td>{{ formatDate(sync.lastSync, locale) }}</td>
          <td>
            <div class="app_toolbar">
              <button
                class="app_button app_button--small"
                :data-testid="`detail_${sync.id}`"
                @click="emit('detail', sync.id)"
              >
                {{ t.details }}</button
              ><button
                class="app_button app_button--small"
                :disabled="busy || sync.revoked"
                :data-testid="`pause_${sync.id}`"
                @click="emit('pause', sync)"
              >
                {{ sync.paused ? t.resume : t.pause }}</button
              ><button
                class="app_button app_button--small"
                :disabled="busy || sync.revoked"
                :data-testid="`revoke_${sync.id}`"
                @click="emit('revoke', sync.id)"
              >
                {{ t.revoke }}
              </button>
            </div>
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</template>

<style scoped lang="scss">

th,
td {
  padding: $sync_space_small $sync_space;
  vertical-align: top;
  white-space: nowrap;
}

.app_toolbar {
  flex-wrap: nowrap;
}

progress {
  display: block;
  max-width: 100%;
  accent-color: var(--nas-accent);
}

</style>
