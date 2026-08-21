<script lang="ts">
  import { onDestroy, onMount, tick } from 'svelte';
  import Composer from '../components/Composer.svelte';
  import Message from '../components/Message.svelte';
  import Icon from '../components/Icon.svelte';
  import TaskCapsule from '../components/TaskCapsule.svelte';
  import type { Locale, MessageAttachment, SessionSnapshot } from '../types';
  import { translate } from '../i18n';
  import { distanceFromBottom, pinToBottom, SCROLL_FOLLOW_THRESHOLD_PX, shouldFollowAfterScroll } from '../scrollFollow';

  export let locale: Locale;
  export let session: SessionSnapshot;
  export let userAvatar: string | undefined = undefined;
  export let disabled = false;
  export let imageInput = false;
  export let taskPanelOpen = false;
  export let onOpenTask: (() => void) | undefined = undefined;
  export let onSend: (content: string, clarify?: boolean, attachments?: MessageAttachment[]) => Promise<void>;
  export let onStop: () => Promise<void>;

  let stream: HTMLDivElement;
  let feed: HTMLDivElement;
  let resizeObserver: ResizeObserver;
  let mutationObserver: MutationObserver;
  let scrollFrame = 0;
  let following = true;
  let smoothFollowRequested = false;
  let previousScrollTop = 0;
  let observedSessionId = session.id;
  let observedLastMessageId = session.messages[session.messages.length - 1]?.id;

  $: t = (key: string) => translate(locale, key);
  $: streaming = session.status === 'streaming';
  $: failure = session.lastError
    ? translate(locale, session.lastError.key, session.lastError.args ?? {})
    : t('conversation.failed');
  $: {
    const latestMessageId = session.messages[session.messages.length - 1]?.id;
    if (session.id !== observedSessionId) {
      observedSessionId = session.id;
      observedLastMessageId = latestMessageId;
      following = true;
      void tick().then(() => {
        if (!stream) return;
        previousScrollTop = stream.scrollTop;
        scheduleFollow();
      });
    } else if (latestMessageId !== observedLastMessageId) {
      const continueFollowing = following;
      observedLastMessageId = latestMessageId;
      if (continueFollowing) void tick().then(() => scheduleFollow(true));
    }
  }

  function metrics() {
    return {
      scrollTop: stream.scrollTop,
      scrollHeight: stream.scrollHeight,
      clientHeight: stream.clientHeight,
    };
  }

  function scheduleFollow(smooth = false) {
    smoothFollowRequested ||= smooth;
    if (!following || !stream || scrollFrame) return;
    scrollFrame = requestAnimationFrame(() => {
      scrollFrame = 0;
      if (!following || !stream) return;
      const useSmoothScroll = smoothFollowRequested
        && !window.matchMedia('(prefers-reduced-motion: reduce)').matches;
      smoothFollowRequested = false;
      if (useSmoothScroll) stream.scrollTo({ top: stream.scrollHeight, behavior: 'smooth' });
      else pinToBottom(stream);
      previousScrollTop = stream.scrollTop;
    });
  }

  function handleScroll() {
    const current = metrics();
    following = shouldFollowAfterScroll({ following, previousScrollTop, current });
    previousScrollTop = current.scrollTop;
  }

  function detachForUpwardIntent() {
    following = false;
    smoothFollowRequested = false;
  }

  function handleWheel(event: WheelEvent) {
    if (event.deltaY < 0) detachForUpwardIntent();
  }

  async function sendAndFollow(content: string, clarify = false, attachments: MessageAttachment[] = []) {
    const followThisTurn = Boolean(stream)
      && following
      && distanceFromBottom(metrics()) <= SCROLL_FOLLOW_THRESHOLD_PX;
    await onSend(content, clarify, attachments);
    if (!followThisTurn) return;
    following = true;
    await tick();
    scheduleFollow(true);
  }

  onMount(() => {
    previousScrollTop = stream.scrollTop;
    resizeObserver = new ResizeObserver(() => scheduleFollow());
    resizeObserver.observe(stream);
    resizeObserver.observe(feed);
    mutationObserver = new MutationObserver(() => scheduleFollow());
    mutationObserver.observe(feed, { childList: true, characterData: true, subtree: true });
    stream.addEventListener('scroll', handleScroll, { passive: true });
    stream.addEventListener('wheel', handleWheel, { passive: true });
    scheduleFollow();
  });

  onDestroy(() => {
    cancelAnimationFrame(scrollFrame);
    resizeObserver?.disconnect();
    mutationObserver?.disconnect();
    stream?.removeEventListener('scroll', handleScroll);
    stream?.removeEventListener('wheel', handleWheel);
  });
</script>

<section class:has-task={Boolean(session.task?.total && onOpenTask)} class="conversation-page">
  <div class="conversation-herald" aria-hidden="true">
    <img src="/brand/knightframe-ui-hero-white.png" alt="" />
  </div>
  <div
    class="conversation-stream"
    bind:this={stream}
    role="log"
    aria-label={t('conversation.timeline')}
    aria-live="polite"
  >
    <div class="conversation-feed" bind:this={feed}>
      {#each session.messages as message, index (message.id)}
        <Message {locale} {message} {userAvatar} streaming={streaming && index === session.messages.length - 1 && message.role === 'assistant'} />
      {/each}
      {#if streaming && session.messages[session.messages.length - 1]?.role !== 'assistant'}
        <div class="waiting-response"><span></span><span></span><span></span><em>{t('conversation.streaming')}</em></div>
      {/if}
      {#if session.status === 'failed'}
        <div class="conversation-error"><Icon name="alert" /><span><strong>{failure}</strong><small>{t('conversation.failed.hint')}</small></span></div>
      {/if}
    </div>
  </div>
  <div class="composer-dock">
    {#if onOpenTask}<TaskCapsule {locale} {session} panelOpen={taskPanelOpen} onOpen={onOpenTask} />{/if}
    <Composer {locale} {streaming} {disabled} {imageInput} onSend={sendAndFollow} {onStop} />
  </div>
</section>
