<script setup lang="ts">

import { NasEmptyState } from '@valeriogc/nas-base-app';
import { useI18n } from '@/i18n';
import { formatDate } from '@/services/sync';
import type { Device } from '@/types/sync';

import devicesIcon from '@/assets/icons/devices.svg?url';

interface Props {
  devices: Device[];
  busy: boolean;
}

defineProps<Props>();

const emit = defineEmits<{ revoke: [id: string] }>();

const { t, locale } = useI18n();

</script>

<template>
  <NasEmptyState
    v-if="!devices.length"
    class="nas_empty_framed"
    :title="t.emptyDevices"
    :description="t.emptyDevicesNote"
    data-testid="device_empty"
  >
    <template #icon>
      <span class="nas_empty_badge" aria-hidden="true">
        <img class="app_icon" :src="devicesIcon" alt="" />
      </span>
    </template>
  </NasEmptyState>

  <div v-else class="app_scroll">
    <table>
      <thead>
        <tr>
          <th>{{ t.computer }}</th>
          <th>{{ t.state }}</th>
          <th>{{ t.lastSeen }}</th>
          <th>{{ t.version }}</th>
          <th>{{ t.syncs }}</th>
          <th>{{ t.actions }}</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="device in devices" :key="device.id">
          <td>
            {{ device.name }}<small class="device_identity">{{ device.id }}</small>
          </td>
          <td>{{ device.online ? t.online : t.offline }}</td>
          <td>{{ formatDate(device.lastSeen, locale) }}</td>
          <td>{{ device.clientVersion }}</td>
          <td>{{ device.syncs }}</td>
          <td>
            <button
              class="app_button app_button--small"
              :disabled="busy"
              :data-testid="`device_revoke_${device.id}`"
              @click="emit('revoke', device.id)"
            >
              {{ t.revoke }}
            </button>
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</template>

<style scoped lang="scss">

th,
td {
  padding: $sync_space;
  vertical-align: top;
}

.device_identity {
  display: block;
  color: var(--nas-muted);
  overflow-wrap: anywhere;
}

</style>
