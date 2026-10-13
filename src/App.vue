<script setup lang="ts">

import { computed, onMounted, onUnmounted, ref } from 'vue';
import {
  NasAppLayout,
  NasConfirmDialog,
  NasLoading,
  type NasCategory,
} from '@valeriogc/nas-base-app';
import { useNasLauncherContext } from '@/composables/useNasLauncherContext';
import { syncApi } from '@/services/sync';
import SyncList from '@/components/SyncList.vue';
import DeviceList from '@/components/DeviceList.vue';
import SyncDetail from '@/components/SyncDetail.vue';
import SettingsDialog from '@/components/SettingsDialog.vue';
import devicesIcon from '@/assets/icons/devices.svg?url';
import errorIcon from '@/assets/icons/error.svg?url';
import infoIcon from '@/assets/icons/info.svg?url';
import refreshIcon from '@/assets/icons/refresh.svg?url';
import settingsIcon from '@/assets/icons/settings.svg?url';
import syncIcon from '@/assets/icons/sync.svg?url';
import type {
  Device,
  ListedSync,
  Section,
  Status,
  SyncDetail as Detail,
  Volume,
} from '@/types/sync';
const { t } = useNasLauncherContext();
const section = ref<Section>('syncs');
const syncs = ref<ListedSync[]>([]);
const devices = ref<Device[]>([]);
const volumes = ref<Volume[]>([]);
const status = ref<Status | null>(null);
const detail = ref<Detail | null>(null);
const selected = ref<string | null>(null);
const settingsOpen = ref(false);
const error = ref('');
const loading = ref(true);
const busy = ref(false);
const pending = ref<{
  kind: 'sync' | 'device' | 'conflict';
  id: string;
  path?: string;
  keep?: 'this' | 'other';
} | null>(null);
const confirmTitle = computed(() =>
  pending.value?.kind === 'device'
    ? t.value.revokeDeviceTitle
    : pending.value?.kind === 'conflict'
      ? t.value.conflictTitle
      : t.value.revokeSyncTitle,
);
const confirmMessage = computed(() =>
  pending.value?.kind === 'device'
    ? t.value.revokeDeviceMessage
    : pending.value?.kind === 'conflict'
      ? t.value.conflictMessage
      : t.value.revokeSyncMessage,
);
const confirmAction = computed(() =>
  pending.value?.kind === 'conflict'
    ? pending.value.keep === 'this'
      ? t.value.keepThis
      : t.value.keepOther
    : t.value.revoke,
);
const sections = computed<NasCategory<Section>[]>(() => [
  { value: 'syncs', label: t.value.syncs, icon: syncIcon, testid: 'nav_syncs' },
  { value: 'devices', label: t.value.devices, icon: devicesIcon, testid: 'nav_devices' },
]);
/** What the engine is doing when it does not run: missing, failing, or still starting. */
const engine = computed(() => {
  const value = status.value;
  if (!value || value.running) {
    return null;
  }
  if (!value.installed) {
    return { tone: 'error', icon: errorIcon, ...t.value.engine.missing };
  }
  return value.failing
    ? { tone: 'error', icon: errorIcon, ...t.value.engine.failing }
    : { tone: 'info', icon: infoIcon, ...t.value.engine.starting };
});
let timer: ReturnType<typeof setInterval> | undefined;
let refreshing = false;
let alive = true;
async function refresh() {
  if (refreshing) {
    return;
  }
  refreshing = true;
  const id = selected.value;
  try {
    const [nextStatus, nextSyncs, nextDevices, nextVolumes, nextDetail] = await Promise.all([
      syncApi.status(),
      syncApi.syncs(),
      syncApi.devices(),
      syncApi.volumes(),
      id ? syncApi.detail(id) : Promise.resolve(null),
    ]);
    if (!alive) {
      return;
    }
    status.value = nextStatus;
    syncs.value = nextSyncs;
    devices.value = nextDevices;
    volumes.value = nextVolumes;
    if (id === selected.value) {
      detail.value = nextDetail;
    }
    error.value = '';
  } catch (failure) {
    if (alive) {
      error.value = String(failure);
    }
  } finally {
    refreshing = false;
    loading.value = false;
  }
}
function navigate(value: Section) {
  section.value = value;
  selected.value = null;
  detail.value = null;
}
async function showDetail(id: string) {
  selected.value = id;
  detail.value = null;
  await refresh();
}
async function run(action: () => Promise<unknown>) {
  if (busy.value) {
    return;
  }
  busy.value = true;
  error.value = '';
  try {
    await action();
    pending.value = null;
    await refresh();
  } catch (failure) {
    error.value = String(failure);
  } finally {
    busy.value = false;
  }
}
async function pause(sync: ListedSync) {
  await run(() => syncApi.pause(sync.id, !sync.paused));
}
async function confirm() {
  const value = pending.value;
  if (!value) {
    return;
  }
  await run(async () => {
    if (value.kind === 'sync') {
      await syncApi.revoke(value.id);
    } else if (value.kind === 'device') {
      await syncApi.revokeDevice(value.id);
      selected.value = null;
      detail.value = null;
    } else if (value.path && value.keep) {
      await syncApi.resolve(value.id, value.path, value.keep);
    }
  });
}
onMounted(() => {
  void refresh();
  timer = setInterval(() => {
    if (!busy.value && !settingsOpen.value && !pending.value) {
      void refresh();
    }
  }, 5000);
});
onUnmounted(() => {
  alive = false;
  clearInterval(timer);
});

</script>

<template>
  <NasAppLayout
    :model-value="section"
    :categories="sections"
    :navigation-label="t.sectionsLabel"
    @update:model-value="navigate"
  >
    <template #icon="{ category }">
      <img class="app_icon category_icon" :src="category.icon" alt="" aria-hidden="true" />
    </template>
    <template #sidebar-footer>
      <button
        type="button"
        class="category_button settings_item"
        data-testid="nav_settings"
        @click="settingsOpen = true"
      >
        <img class="app_icon category_icon" :src="settingsIcon" alt="" aria-hidden="true" />
        {{ t.settings }}
      </button>
    </template>
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
          @click="refresh"
        >
          <img class="app_icon" :src="refreshIcon" alt="" aria-hidden="true" />
        </button>
      </div>
      <p>{{ section === 'syncs' ? t.intro : t.devicesIntro }}</p>
    </header>
    <div v-if="error" class="status_banner status_banner_error" role="alert">
      <img class="app_icon" :src="errorIcon" alt="" aria-hidden="true" />
      <span class="status_text">
        <strong>{{ t.loadFailed }}</strong>
        {{ error }}
      </span>
      <button
        type="button"
        class="app_button app_button--small"
        data-testid="retry"
        @click="refresh"
      >
        {{ t.retry }}
      </button>
    </div>
    <div
      v-if="engine"
      :class="[
        'status_banner',
        {
          status_banner_error: engine.tone === 'error',
          status_banner_info: engine.tone === 'info',
        },
      ]"
      :role="engine.tone === 'error' ? 'alert' : 'status'"
      data-testid="engine_status"
    >
      <img class="app_icon" :src="engine.icon" alt="" aria-hidden="true" />
      <span class="status_text">
        <strong>{{ engine.title }}</strong>
        {{ engine.text }}
      </span>
    </div>
    <NasLoading v-if="loading || (selected && !detail && !error)" :label="t.loading" />
    <SyncDetail
      v-else-if="detail"
      :detail="detail"
      :volumes="volumes"
      :busy="busy"
      @close="
        selected = null;
        detail = null;
      "
      @pause="pause(detail.sync)"
      @revoke="pending = { kind: 'sync', id: detail.sync.id }"
      @resolve="(path, keep) => (pending = { kind: 'conflict', id: detail!.sync.id, path, keep })"
    />
    <SyncList
      v-else-if="section === 'syncs'"
      :syncs="syncs"
      :volumes="volumes"
      :busy="busy"
      @detail="showDetail"
      @pause="pause"
      @revoke="(id) => (pending = { kind: 'sync', id })"
    />
    <DeviceList
      v-else
      :devices="devices"
      :busy="busy"
      @revoke="(id) => (pending = { kind: 'device', id })"
    />
    <p v-if="status?.deviceId" class="device_identity">
      <span>{{ t.identity }}</span>
      <code>{{ status.deviceId }}</code>
    </p>
    <!-- Inside the layout, where the shared styles of the controls and the toolbars apply. -->
    <SettingsDialog v-if="settingsOpen" @close="settingsOpen = false" @saved="refresh" />
    <NasConfirmDialog
      v-if="pending"
      :title="confirmTitle"
      :message="confirmMessage"
      :cancel-label="t.cancel"
      :confirm-label="confirmAction"
      :busy="busy"
      cancel-testid="confirm_cancel"
      confirm-testid="confirm_action"
      @cancel="pending = null"
      @confirm="confirm"
      ><p v-if="error" class="app_error" role="alert">{{ error }}</p></NasConfirmDialog
    >
  </NasAppLayout>
</template>

<style scoped lang="scss">

// Down at the bottom, apart from the sections.
.settings_item {
  margin-top: auto;
}

.page_header {
  margin-bottom: $sync_space_large;
}

// The title, with the refresh on the right.
.page_heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: $sync_space;
}

.status_banner {
  --app-icon-size: 18px;
  --status-tone: var(--tone-info);
  display: flex;
  align-items: flex-start;
  gap: $sync_space_small + 2px;
  margin-bottom: $sync_space;
  padding: 12px 14px;
  border: 1px solid color-mix(in srgb, var(--status-tone) 45%, transparent);
  border-radius: $sync_radius;
  background: color-mix(in srgb, var(--status-tone) 10%, var(--nas-surface));
  line-height: 1.45;

  .app_icon {
    margin-top: 2px;
  }

  strong {
    display: block;
  }

  .app_button {
    margin-left: auto;
  }
}

.status_banner_error {
  --status-tone: var(--tone-error);
}

.status_banner_info {
  --status-tone: var(--tone-info);
}

.status_text {
  min-width: 0;
  overflow-wrap: anywhere;
}

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
