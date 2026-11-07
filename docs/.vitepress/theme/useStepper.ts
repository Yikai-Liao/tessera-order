// SPDX-License-Identifier: BSD-2-Clause
import { computed, onBeforeUnmount, type Ref, ref, watch } from "vue";

/** Frame index, play/pause and speed for a step-through animation. */
export function useStepper(count: Ref<number>) {
  const index = ref(0);
  const playing = ref(false);
  const speed = ref(1);
  let timer: ReturnType<typeof setInterval> | undefined;

  const last = computed(() => Math.max(0, count.value - 1));
  const stop = () => {
    playing.value = false;
    if (timer) clearInterval(timer);
    timer = undefined;
  };
  const play = () => {
    if (index.value >= last.value) index.value = 0;
    playing.value = true;
    if (timer) clearInterval(timer);
    timer = setInterval(() => {
      if (index.value >= last.value) stop();
      else index.value++;
    }, 700 / speed.value);
  };
  const toggle = () => (playing.value ? stop() : play());
  const step = (delta: number) => {
    stop();
    index.value = Math.min(last.value, Math.max(0, index.value + delta));
  };
  const seek = (to: number) => {
    stop();
    index.value = Math.min(last.value, Math.max(0, to));
  };
  watch(speed, () => playing.value && play());
  watch(count, () => seek(0));
  onBeforeUnmount(stop);
  return { index, playing, speed, last, play, stop, toggle, step, seek };
}
