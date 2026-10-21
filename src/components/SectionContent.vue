<script setup lang="ts">

import { NasLoading } from '@valeriogc/nas-base-app';
import { useI18n } from '@/i18n';
import SyncList from '@/components/SyncList.vue';
import DeviceList from '@/components/DeviceList.vue';
import SyncDetail from '@/components/SyncDetail.vue';
import type {
  Device,
  ListedSync,
  PendingAction,
  Section,
  SyncDetail as Detail,
  Volume,
} from '@/types/sync';

interface Props {
  section: Section;
  loading: boolean;
  detail: Detail | null;
  syncs: ListedSync[];
  devices: Device[];
  volumes: Volume[];
  busy: boolean;
  deviceId: string | null;
}

defineProps<Props>();

const emit = defineEmits<{
  close: [];
  detail: [id: string];
  pause: [sync: ListedSync];
  request: [action: PendingAction];
}>();

const { t } = useI18n();

</script>

<template>
  <NasLoading v-if="loading" :label="t.loading" />

  <!-- Sync detail -->
  <SyncDetail
    v-else-if="detail"
    :detail="detail"
    :volumes="volumes"
    :busy="busy"
    @close="emit('close')"
    @pause="emit('pause', detail.sync)"
    @revoke="emit('request', { kind: 'sync', id: detail.sync.id })"
    @resolve="
      (path, keep) => emit('request', { kind: 'conflict', id: detail!.sync.id, path, keep })
    "
  />
  <SyncList
    v-else-if="section === 'syncs'"
    :syncs="syncs"
    :volumes="volumes"
    :busy="busy"
    @detail="(id) => emit('detail', id)"
    @pause="(sync) => emit('pause', sync)"
    @revoke="(id) => emit('request', { kind: 'sync', id })"
  />
  <DeviceList
    v-else
    :devices="devices"
    :busy="busy"
    @revoke="(id) => emit('request', { kind: 'device', id })"
  />
  <p v-if="deviceId" class="device_identity">
    <span>{{ t.identity }}</span>
    <code>{{ deviceId }}</code>
  </p>
</template>

<style scoped lang="scss">

.device_identity {
  display: grid;
  gap: 4px;
  margin-top: $sync_space_large;
  color: var(--nas-muted);
  font-size: 0.875rem;

  code {
    color: var(--nas-foreground);
    overflow-wrap: anywhere;
  }
}

</style>
