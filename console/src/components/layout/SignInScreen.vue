<script setup lang="ts">
/** The sign-in screen for a 401: paste the token stretto-console printed when it started. */
import { ref } from 'vue'
import { Eye, EyeOff, KeyRound, LogIn } from '@lucide/vue'
import BrandLogo from '../BrandLogo.vue'
import UiButton from '../ui/UiButton.vue'
import { signIn } from '@/stores/auth'

const token = ref('')
const show = ref(false)
const error = ref<string | null>(null)
const busy = ref(false)

async function submit() {
  busy.value = true
  error.value = await signIn(token.value)
  busy.value = false
}
</script>

<template>
  <main class="signin">
    <form class="signin-card" data-testid="sign-in" @submit.prevent="submit">
      <BrandLogo :height="26" />
      <div class="signin-head">
        <h1>Sign in to the console</h1>
        <p class="muted">
          When it starts, <span class="mono">stretto-console</span> prints a link with its token,
          such as <span class="mono">http://127.0.0.1:7878/?token=…</span>. Open that link, or paste
          the token here.
        </p>
      </div>
      <label class="signin-label" for="token">Token</label>
      <div class="signin-input">
        <KeyRound class="signin-key" :size="16" :stroke-width="1.9" aria-hidden="true" />
        <input
          id="token"
          v-model="token"
          class="input mono"
          :type="show ? 'text' : 'password'"
          autocomplete="off"
          spellcheck="false"
          autofocus
          :aria-invalid="!!error"
          aria-describedby="token-help"
          placeholder="64 hex digits"
        />
        <button
          type="button"
          class="signin-eye"
          :aria-label="show ? 'Hide the token' : 'Show the token'"
          @click="show = !show"
        >
          <EyeOff v-if="show" :size="16" :stroke-width="1.9" aria-hidden="true" />
          <Eye v-else :size="16" :stroke-width="1.9" aria-hidden="true" />
        </button>
      </div>
      <p v-if="error" class="signin-error" role="alert">{{ error }}</p>
      <p id="token-help" class="caption">
        The token is also in <span class="mono">STRETTO_CONSOLE_TOKEN</span> if you set it, or after
        <span class="mono">--token</span>. The browser keeps it in a cookie only this console reads.
      </p>
      <UiButton type="submit" variant="primary" :icon="LogIn" :loading="busy" block
        >Sign in</UiButton
      >
    </form>
  </main>
</template>

<style scoped>
.signin {
  display: grid;
  place-items: center;
  min-height: 100vh;
  padding: 24px 16px;
  background:
    radial-gradient(
      60% 50% at 50% 0%,
      color-mix(in oklab, var(--stretto-accent-soft) 90%, transparent),
      transparent 70%
    ),
    var(--stretto-bg);
}

.signin-card {
  display: flex;
  flex-direction: column;
  gap: 14px;
  width: min(440px, 100%);
  padding: 28px;
  border-radius: 16px;
  background: var(--stretto-surface);
  border: 1px solid var(--stretto-border);
  box-shadow: var(--c-shadow-md);
}

.signin-head h1 {
  margin: 8px 0 8px;
  font-size: 20px;
  letter-spacing: -0.015em;
}

.signin-head p {
  font-size: 13.5px;
}

.signin-head .mono {
  font-size: 12.5px;
  overflow-wrap: anywhere;
}

.signin-label {
  font-size: 13px;
  font-weight: 500;
  margin-bottom: -6px;
}

.signin-input {
  position: relative;
}

.signin-input .input {
  padding-left: 36px;
  padding-right: 40px;
}

.signin-key {
  position: absolute;
  left: 12px;
  top: 10px;
  color: var(--stretto-text-subtle);
}

.signin-eye {
  position: absolute;
  right: 4px;
  top: 3px;
  display: grid;
  place-items: center;
  width: 30px;
  height: 30px;
  border: 0;
  border-radius: 7px;
  background: none;
  color: var(--stretto-text-subtle);
}

.signin-eye:hover {
  background: var(--c-hover);
  color: var(--stretto-text);
}

.signin-error {
  font-size: 13px;
  color: var(--c-danger);
}

.caption .mono {
  font-size: 11.5px;
}
</style>
