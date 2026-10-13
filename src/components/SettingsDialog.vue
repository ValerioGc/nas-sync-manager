<script setup lang="ts">

import { onMounted, ref, useId } from 'vue';
import { NasLoading, NasModal, NasNumberStepper } from '@valeriogc/nas-base-app';
import { useI18n } from '@/i18n';
import { syncApi } from '@/services/sync';
import type { Settings } from '@/types/sync';

// The bounds the backend accepts: the steppers keep the numbers inside them.
const MAX_KBPS = 10_000_000;
const MAX_RESCAN_MINUTES = 1440;

const emit = defineEmits<{ close: []; saved: [] }>();
const { t } = useI18n();
const settings = ref<Settings | null>(null);
const error = ref('');
const busy = ref(false);
const sendId = useId();
const receiveId = useId();
const rescanId = useId();

async function load() {
  error.value = '';
  try {
    settings.value = await syncApi.settings();
  } catch (error_) {
    error.value = String(error_);
  }
}
onMounted(load);
async function save() {
  if (!settings.value || busy.value) {
    return;
  }
  busy.value = true;
  error.value = '';
  try {
    await syncApi.saveSettings(settings.value);
    emit('saved');
    emit('close');
  } catch (error_) {
    error.value = String(error_);
  } finally {
    busy.value = false;
  }
}

</script>

<template>
  <!-- The native dialog of the library: Escape and a click beside the panel close it. -->
  <NasModal :label="t.settings" :dismissible="!busy" @close="emit('close')">
    <section class="settings_panel">
      <h2>{{ t.settings }}</h2>
      <div class="settings_body">
        <p v-if="error" class="app_error" role="alert">
          {{ error }}
          <button v-if="!settings" type="button" class="app_button app_button--small" @click="load">
            {{ t.retry }}
          </button>
        </p>
        <form v-if="settings" id="settings_form" class="settings_form" @submit.prevent="save">
          <div class="field">
            <label class="field_label" :for="sendId">{{ t.settingsModal.send }}</label>
            <NasNumberStepper
              :id="sendId"
              v-model="settings.maxSendKbps"
              class="field_number"
              :min="0"
              :max="MAX_KBPS"
              :increase-label="t.increase"
              :decrease-label="t.decrease"
              testid="settings_send"
            />
          </div>
          <div class="field">
            <label class="field_label" :for="receiveId">{{ t.settingsModal.receive }}</label>
            <NasNumberStepper
              :id="receiveId"
              v-model="settings.maxRecvKbps"
              class="field_number"
              :min="0"
              :max="MAX_KBPS"
              :increase-label="t.increase"
              :decrease-label="t.decrease"
              testid="settings_receive"
            />
            <small>{{ t.settingsModal.unlimited }}</small>
          </div>
          <div class="field">
            <label class="field_label" :for="rescanId">{{ t.settingsModal.rescan }}</label>
            <NasNumberStepper
              :id="rescanId"
              v-model="settings.rescanMinutes"
              class="field_number"
              :min="1"
              :max="MAX_RESCAN_MINUTES"
              :increase-label="t.increase"
              :decrease-label="t.decrease"
              testid="settings_rescan"
            />
            <small>{{ t.settingsModal.rescanHint }}</small>
          </div>
        </form>
        <NasLoading v-else-if="!error" :label="t.loading" />
      </div>
      <footer class="settings_footer app_toolbar">
        <button
          type="button"
          class="app_button"
          :disabled="busy"
          data-testid="settings_close"
          @click="emit('close')"
        >
          {{ settings ? t.cancel : t.close }}
        </button>
        <button
          v-if="settings"
          type="submit"
          form="settings_form"
          class="app_button app_button--primary"
          :disabled="busy"
          data-testid="settings_save"
        >
          {{ t.save }}
        </button>
      </footer>
    </section>
  </NasModal>
</template>

<style scoped lang="scss">

// Title and buttons stay in place: only the body scrolls, inside the rounded edge.
.settings_panel {
  display: flex;
  flex-direction: column;
  width: min(100%, 520px);
  max-height: calc(100vh - 40px);
  overflow: hidden;
  border: 1px solid var(--nas-border);
  border-radius: 14px;
  background: var(--nas-surface);

  h2 {
    flex: none;
    margin: 0;
    padding: 20px 24px 0;
    font-size: 1.15rem;
  }
}

.settings_body {
  min-height: 0;
  padding: 16px 24px 20px;
  overflow: auto;
}

.settings_form {
  display: grid;
  gap: 20px;
}

.field {
  display: grid;
  justify-items: start;
  gap: 6px;

  small {
    color: var(--nas-muted);
  }
}

.field_label {
  font-weight: 600;
}

.field_number {
  width: 180px;
}

.settings_footer {
  flex: none;
  justify-content: flex-end;
  padding: 0 24px 20px;
}

</style>
