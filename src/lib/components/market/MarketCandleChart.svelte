<script lang="ts">
  import { onMount } from 'svelte';
  import type { KlineFrame } from '../../types';

  export let frame: KlineFrame;

  let canvas: HTMLCanvasElement;
  let wrapper: HTMLDivElement;

  function draw(): void {
    if (!canvas || !wrapper) return;
    const dpr = window.devicePixelRatio || 1;
    const width = wrapper.clientWidth;
    const height = wrapper.clientHeight;
    if (width === 0 || height === 0) return;
    canvas.width = Math.round(width * dpr);
    canvas.height = Math.round(height * dpr);
    canvas.style.width = `${width}px`;
    canvas.style.height = `${height}px`;
    const context = canvas.getContext('2d');
    if (!context) return;
    context.setTransform(dpr, 0, 0, dpr, 0, 0);
    context.clearRect(0, 0, width, height);

    // bars[0] 最新 → 反转为左旧右新
    const bars = [...frame.bars].reverse();
    if (!bars.length) return;
    const ema = [...(frame.indicators.ema20 ?? [])].reverse();

    const padLeft = 10;
    const padRight = 74;
    const padTop = 14;
    const padBottom = 24;
    const plotWidth = Math.max(10, width - padLeft - padRight);
    const plotHeight = Math.max(10, height - padTop - padBottom);

    let min = Infinity;
    let max = -Infinity;
    for (const bar of bars) {
      if (bar.low < min) min = bar.low;
      if (bar.high > max) max = bar.high;
    }
    for (const value of ema) {
      if (typeof value === 'number' && value > 0) {
        if (value < min) min = value;
        if (value > max) max = value;
      }
    }
    if (!Number.isFinite(min) || !Number.isFinite(max)) return;
    const span = max - min || Math.max(max, 1) * 0.01;
    min -= span * 0.06;
    max += span * 0.06;

    const n = bars.length;
    const slot = plotWidth / n;
    const bodyWidth = Math.max(1, Math.min(13, slot * 0.62));
    const yOf = (price: number): number => padTop + plotHeight - ((price - min) / (max - min)) * plotHeight;
    const xOf = (index: number): number => padLeft + slot * (index + 0.5);

    // 网格（4 条水平线）
    context.strokeStyle = 'rgba(255,255,255,.05)';
    context.lineWidth = 1;
    context.font = '10px "JetBrains Mono", Consolas, monospace';
    context.fillStyle = '#5c5c5c';
    context.textBaseline = 'middle';
    for (let line = 0; line <= 4; line += 1) {
      const price = min + ((max - min) * line) / 4;
      const y = yOf(price);
      context.beginPath();
      context.moveTo(padLeft, y);
      context.lineTo(padLeft + plotWidth, y);
      context.stroke();
      context.fillText(price.toFixed(price > 100 ? 1 : 3), padLeft + plotWidth + 8, y);
    }

    // EMA20
    context.strokeStyle = 'rgba(255,255,255,.34)';
    context.lineWidth = 1;
    context.setLineDash([4, 3]);
    context.beginPath();
    let started = false;
    ema.forEach((value, index) => {
      if (typeof value !== 'number' || !Number.isFinite(value)) return;
      const x = xOf(index);
      const y = yOf(value);
      if (!started) {
        context.moveTo(x, y);
        started = true;
      } else {
        context.lineTo(x, y);
      }
    });
    context.stroke();
    context.setLineDash([]);

    // 蜡烛
    for (let index = 0; index < n; index += 1) {
      const bar = bars[index];
      const x = xOf(index);
      const bull = bar.close >= bar.open;
      const alpha = bar.closed ? 1 : 0.45;
      context.globalAlpha = alpha;
      if (bull) {
        context.strokeStyle = '#ededed';
        context.fillStyle = '#ededed';
      } else {
        context.strokeStyle = '#7c7c7c';
        context.fillStyle = 'rgba(255,255,255,.02)';
      }
      context.lineWidth = 1;
      // 影线
      context.beginPath();
      context.moveTo(Math.round(x) + 0.5, yOf(bar.high));
      context.lineTo(Math.round(x) + 0.5, yOf(bar.low));
      context.stroke();
      // 实体
      const top = yOf(Math.max(bar.open, bar.close));
      const bottom = yOf(Math.min(bar.open, bar.close));
      const bodyHeight = Math.max(1, bottom - top);
      const bodyX = Math.round(x - bodyWidth / 2) + 0.5;
      context.beginPath();
      context.rect(bodyX, Math.round(top), Math.round(bodyWidth), Math.round(bodyHeight));
      if (bull) context.fill();
      else context.stroke();
      context.globalAlpha = 1;
    }

    // 最新价虚线
    const last = bars[n - 1];
    const lastY = yOf(last.close);
    context.strokeStyle = 'rgba(255,255,255,.4)';
    context.setLineDash([3, 4]);
    context.beginPath();
    context.moveTo(padLeft, lastY);
    context.lineTo(padLeft + plotWidth, lastY);
    context.stroke();
    context.setLineDash([]);
    context.fillStyle = '#0c0c0c';
    context.strokeStyle = '#e6e6e6';
    context.beginPath();
    const label = last.close.toFixed(last.close > 100 ? 1 : 3);
    const labelWidth = context.measureText(label).width + 12;
    context.roundRect(padLeft + plotWidth + 3, lastY - 9, labelWidth, 18, 5);
    context.fill();
    context.stroke();
    context.fillStyle = '#eee';
    context.fillText(label, padLeft + plotWidth + 9, lastY);

    // 底部时间标签（首 / 中 / 尾）
    context.fillStyle = '#5c5c5c';
    const timeOf = (bar: typeof last): string => {
      const date = new Date(bar.tsOpen);
      return `${String(date.getHours()).padStart(2, '0')}:${String(date.getMinutes()).padStart(2, '0')}`;
    };
    context.textAlign = 'left';
    context.fillText(timeOf(bars[0]), padLeft, height - padBottom / 2);
    context.textAlign = 'center';
    context.fillText(timeOf(bars[Math.floor(n / 2)]), xOf(Math.floor(n / 2)), height - padBottom / 2);
    context.textAlign = 'right';
    context.fillText(timeOf(last), padLeft + plotWidth, height - padBottom / 2);
    context.textAlign = 'left';
  }

  $: if (canvas && frame) draw();
  onMount(() => {
    const observer = new ResizeObserver(() => draw());
    observer.observe(wrapper);
    return () => observer.disconnect();
  });
</script>

<div class="candle-chart" bind:this={wrapper}>
  <canvas bind:this={canvas} aria-label="K line chart"></canvas>
  {#if !frame.bars.length}
    <div class="chart-empty">{frame.symbol}</div>
  {/if}
</div>

<style>
  .candle-chart {
    position: relative;
    width: 100%;
    height: 100%;
    min-height: 220px;
    overflow: hidden;
    border: 1px solid var(--border-soft);
    border-radius: var(--radius-md);
    background:
      radial-gradient(ellipse at 78% -18%, rgba(255, 255, 255, 0.035), transparent 62%),
      #0a0a0a;
  }
  canvas {
    display: block;
    width: 100%;
    height: 100%;
  }
  .chart-empty {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    color: var(--dim);
    font: 500 12px var(--mono);
    letter-spacing: 0.12em;
  }
</style>
