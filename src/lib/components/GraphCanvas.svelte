<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import type { GraphSnapshot } from '../types';

  export let graph: GraphSnapshot;
  export let query = '';
  export let showFiles = true;
  export let showDirectories = true;
  export let nodeScale = 1;
  export let linkStrength = 1;
  export let repulsion = 1;
  export let label = '';

  type SimNode = GraphSnapshot['nodes'][number] & {
    x: number; y: number; born: number; index: number;
  };
  type CameraView = { panX: number; panY: number; scale: number };
  type CameraMotion = { from: CameraView; to: CameraView; startedAt: number; duration: number };

  const DRAG_THRESHOLD = 5;
  const CAMERA_DURATION = 520;

  let canvas: HTMLCanvasElement;
  let host: HTMLDivElement;
  let context: CanvasRenderingContext2D | null = null;
  let nodes: SimNode[] = [];
  let edges: GraphSnapshot['edges'] = [];
  let byId = new Map<string, SimNode>();
  let animation = 0;
  let observer: ResizeObserver;
  let motionPreference: MediaQueryList;
  let width = 1;
  let height = 1;
  let scale = 1;
  let panX = 0;
  let panY = 0;
  let pointerX = 0;
  let pointerY = 0;
  let pointerId: number | undefined;
  let pressedNode: SimNode | undefined;
  let dragging: SimNode | undefined;
  let panning = false;
  let pointerMoved = false;
  let pressedPointerX = 0;
  let pressedPointerY = 0;
  let lastPointerX = 0;
  let lastPointerY = 0;
  let hover: SimNode | undefined;
  let focus: SimNode | undefined;
  let keyboardNode: SimNode | undefined;
  let canvasFocused = false;
  let returnView: CameraView | undefined;
  let cameraMotion: CameraMotion | undefined;
  let focusAt = 0;
  let startAt = 0;
  let reducedMotion = false;

  $: accessibleLabel = focus
    ? `${label}: ${focus.label}, ${focus.path}`
    : keyboardNode
      ? `${label}: ${keyboardNode.label}, ${keyboardNode.path}`
      : label;

  const hash = (value: string) => {
    let result = 2166136261;
    for (let index = 0; index < value.length; index += 1) result = Math.imul(result ^ value.charCodeAt(index), 16777619);
    return result >>> 0;
  };
  const clamp = (value: number, minimum: number, maximum: number) => Math.max(minimum, Math.min(maximum, value));
  const ease = (value: number) => 1 - Math.pow(1 - clamp(value, 0, 1), 3);
  const nodeRadius = (node: SimNode) => Math.max(1.55, (node.kind === 'directory' ? 3.1 : 1.85) + node.weight * .38) * nodeScale;
  const visible = (node: SimNode) => (node.kind === 'file' ? showFiles : showDirectories)
    && (!query.trim() || `${node.label} ${node.path}`.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()));

  function reset(snapshot = graph, search = query) {
    startAt = performance.now();
    focus = undefined;
    hover = undefined;
    keyboardNode = undefined;
    returnView = undefined;
    cameraMotion = undefined;
    const needle = search.trim().toLocaleLowerCase();
    const candidates = [...snapshot.nodes].sort((left, right) => {
      const leftMatch = needle && `${left.label} ${left.path}`.toLocaleLowerCase().includes(needle) ? 1 : 0;
      const rightMatch = needle && `${right.label} ${right.path}`.toLocaleLowerCase().includes(needle) ? 1 : 0;
      if (leftMatch !== rightMatch) return rightMatch - leftMatch;
      if ((left.kind === 'directory') !== (right.kind === 'directory')) return left.kind === 'directory' ? -1 : 1;
      return right.weight - left.weight || left.id.localeCompare(right.id);
    });
    const grouped = new Map<string, typeof candidates>();
    for (const node of candidates) {
      const group = grouped.get(node.component) ?? [];
      group.push(node);
      grouped.set(node.component, group);
    }
    const groups = [...grouped.entries()].sort(([left, leftNodes], [right, rightNodes]) =>
      rightNodes.length - leftNodes.length || left.localeCompare(right));
    let selected: typeof candidates = [];
    if (needle) {
      selected = candidates.slice(0, 360);
    } else {
      const weight = groups.reduce((total, [, items]) => total + Math.sqrt(items.length), 0) || 1;
      const offsets = new Map<string, number>();
      for (const [component, items] of groups) {
        const allocation = Math.max(1, Math.floor(360 * Math.sqrt(items.length) / weight));
        const take = Math.min(items.length, allocation);
        selected.push(...items.slice(0, take));
        offsets.set(component, take);
      }
      while (selected.length < 360) {
        let added = false;
        for (const [component, items] of groups) {
          const offset = offsets.get(component) ?? 0;
          if (offset < items.length && selected.length < 360) {
            selected.push(items[offset]);
            offsets.set(component, offset + 1);
            added = true;
          }
        }
        if (!added) break;
      }
    }
    const count = Math.max(1, selected.length);
    const componentOrder = [...new Set(selected.map((node) => node.component))];
    const localIndexes = new Map<string, number>();
    nodes = selected.map((node, index) => {
      const seed = hash(node.id);
      const componentIndex = componentOrder.indexOf(node.component);
      const componentCount = componentOrder.length;
      const clusterAngle = componentCount <= 1 ? 0 : componentIndex / componentCount * Math.PI * 2 - Math.PI / 2;
      const clusterRadius = componentCount <= 1 ? 0 : 190 + Math.sqrt(componentCount) * 55;
      const centerX = Math.cos(clusterAngle) * clusterRadius;
      const centerY = Math.sin(clusterAngle) * clusterRadius;
      const localIndex = localIndexes.get(node.component) ?? 0;
      localIndexes.set(node.component, localIndex + 1);
      const angle = ((seed % 10000) / 10000) * Math.PI * 2 + localIndex * 2.39996;
      const spacing = .82 + repulsion * .22;
      const radius = (node.kind === 'directory' ? 22 + Math.sqrt(localIndex + 1) * 8 : 48 + Math.sqrt(localIndex + 1) * 10) * spacing;
      return { ...node, index, x: centerX + Math.cos(angle) * radius, y: centerY + Math.sin(angle) * radius, born: reducedMotion ? 1 : index / count };
    });
    byId = new Map(nodes.map((node) => [node.id, node]));
    edges = snapshot.edges.filter((edge) => byId.has(edge.source) && byId.has(edge.target));
    panX = 0;
    panY = 0;
    scale = 1;
    requestRender();
  }

  function resize() {
    const rectangle = host.getBoundingClientRect();
    const ratio = Math.min(devicePixelRatio || 1, 2);
    width = Math.max(1, rectangle.width);
    height = Math.max(1, rectangle.height);
    canvas.width = Math.round(width * ratio);
    canvas.height = Math.round(height * ratio);
    canvas.style.width = `${width}px`;
    canvas.style.height = `${height}px`;
    context = canvas.getContext('2d');
    context?.setTransform(ratio, 0, 0, ratio, 0, 0);
    requestRender();
  }

  function world(clientX: number, clientY: number) {
    const rectangle = canvas.getBoundingClientRect();
    return {
      x: (clientX - rectangle.left - width / 2 - panX) / scale,
      y: (clientY - rectangle.top - height / 2 - panY) / scale,
    };
  }

  function nearest(clientX: number, clientY: number) {
    const point = world(clientX, clientY);
    let candidate: SimNode | undefined;
    let distance = Number.POSITIVE_INFINITY;
    for (const node of nodes) {
      if (!visible(node)) continue;
      const current = Math.hypot(node.x - point.x, node.y - point.y);
      const hitRadius = Math.max(16 / scale, nodeRadius(node) + 5 / scale);
      if (current <= hitRadius && current < distance) {
        distance = current;
        candidate = node;
      }
    }
    return candidate;
  }

  function relatedNodes(node: SimNode): SimNode[] {
    const ids = new Set([node.id]);
    for (const edge of edges) {
      if (edge.source === node.id) ids.add(edge.target);
      else if (edge.target === node.id) ids.add(edge.source);
    }
    return [...ids].map((id) => byId.get(id)).filter((item): item is SimNode => Boolean(item && visible(item)));
  }

  function focusedView(node: SimNode): CameraView {
    const related = relatedNodes(node);
    let extentX = 50;
    let extentY = 50;
    for (const item of related) {
      extentX = Math.max(extentX, Math.abs(item.x - node.x) * 2 + 70);
      extentY = Math.max(extentY, Math.abs(item.y - node.y) * 2 + 70);
    }
    const fitted = Math.min(width * .68 / extentX, height * .68 / extentY);
    const targetScale = clamp(fitted, 1.25, 2.45);
    return { panX: -node.x * targetScale, panY: -node.y * targetScale, scale: targetScale };
  }

  function advanceCamera(time: number): boolean {
    if (!cameraMotion) return false;
    const progress = clamp((time - cameraMotion.startedAt) / cameraMotion.duration, 0, 1);
    const eased = ease(progress);
    panX = cameraMotion.from.panX + (cameraMotion.to.panX - cameraMotion.from.panX) * eased;
    panY = cameraMotion.from.panY + (cameraMotion.to.panY - cameraMotion.from.panY) * eased;
    scale = cameraMotion.from.scale + (cameraMotion.to.scale - cameraMotion.from.scale) * eased;
    if (progress >= 1) {
      cameraMotion = undefined;
      return false;
    }
    return true;
  }

  function animateCamera(target: CameraView) {
    advanceCamera(performance.now());
    if (reducedMotion) {
      panX = target.panX;
      panY = target.panY;
      scale = target.scale;
      cameraMotion = undefined;
    } else {
      cameraMotion = {
        from: { panX, panY, scale },
        to: target,
        startedAt: performance.now(),
        duration: CAMERA_DURATION,
      };
    }
    requestRender();
  }

  function cancelCamera() {
    if (!cameraMotion) return;
    advanceCamera(performance.now());
    cameraMotion = undefined;
  }

  function toggleFocus(node: SimNode) {
    cancelCamera();
    focusAt = performance.now();
    if (focus?.id === node.id) {
      const target = returnView ?? { panX: 0, panY: 0, scale: 1 };
      focus = undefined;
      returnView = undefined;
      animateCamera(target);
      return;
    }
    if (!focus) returnView = { panX, panY, scale };
    focus = node;
    keyboardNode = node;
    animateCamera(focusedView(node));
  }

  function down(event: PointerEvent) {
    if (event.pointerType === 'mouse' && event.button !== 0) return;
    cancelCamera();
    canvas.focus({ preventScroll: true });
    canvas.setPointerCapture(event.pointerId);
    pointerId = event.pointerId;
    pointerX = event.clientX;
    pointerY = event.clientY;
    pressedPointerX = event.clientX;
    pressedPointerY = event.clientY;
    lastPointerX = event.clientX;
    lastPointerY = event.clientY;
    pressedNode = nearest(event.clientX, event.clientY);
    hover = pressedNode;
    keyboardNode = undefined;
    dragging = undefined;
    panning = false;
    pointerMoved = false;
    requestRender();
  }

  function move(event: PointerEvent) {
    pointerX = event.clientX;
    pointerY = event.clientY;
    if (pointerId === event.pointerId) {
      if (!pointerMoved && Math.hypot(event.clientX - pressedPointerX, event.clientY - pressedPointerY) >= DRAG_THRESHOLD) {
        pointerMoved = true;
        dragging = pressedNode;
        panning = !pressedNode;
      }
      if (dragging) {
        const point = world(event.clientX, event.clientY);
        dragging.x = point.x;
        dragging.y = point.y;
      } else if (panning) {
        panX += event.clientX - lastPointerX;
        panY += event.clientY - lastPointerY;
      }
    } else {
      hover = nearest(event.clientX, event.clientY);
    }
    lastPointerX = event.clientX;
    lastPointerY = event.clientY;
    requestRender();
  }

  function finishPointer(event: PointerEvent, cancelled = false) {
    if (pointerId !== event.pointerId) return;
    if (canvas.hasPointerCapture(event.pointerId)) canvas.releasePointerCapture(event.pointerId);
    const releasedNode = cancelled ? undefined : nearest(event.clientX, event.clientY);
    if (!cancelled && !pointerMoved && pressedNode && releasedNode?.id === pressedNode.id) toggleFocus(pressedNode);
    pointerId = undefined;
    pressedNode = undefined;
    dragging = undefined;
    panning = false;
    pointerMoved = false;
    hover = releasedNode;
    requestRender();
  }

  function leave() {
    if (pointerId !== undefined) return;
    hover = undefined;
    requestRender();
  }

  function wheel(event: WheelEvent) {
    event.preventDefault();
    cancelCamera();
    const before = world(event.clientX, event.clientY);
    scale = clamp(scale * Math.exp(-event.deltaY * .0012), .22, 4.5);
    const rectangle = canvas.getBoundingClientRect();
    panX = event.clientX - rectangle.left - width / 2 - before.x * scale;
    panY = event.clientY - rectangle.top - height / 2 - before.y * scale;
    requestRender();
  }

  function keyboardCandidates(): SimNode[] {
    return (focus ? relatedNodes(focus) : nodes.filter(visible)).sort((left, right) => left.index - right.index);
  }

  function keydown(event: KeyboardEvent) {
    const candidates = keyboardCandidates();
    if (!candidates.length) return;
    if (['ArrowLeft', 'ArrowUp', 'ArrowRight', 'ArrowDown', 'Home', 'End'].includes(event.key)) {
      event.preventDefault();
      const current = keyboardNode ? candidates.findIndex((node) => node.id === keyboardNode?.id) : -1;
      if (event.key === 'Home') keyboardNode = candidates[0];
      else if (event.key === 'End') keyboardNode = candidates[candidates.length - 1];
      else {
        const direction = event.key === 'ArrowLeft' || event.key === 'ArrowUp' ? -1 : 1;
        const start = current >= 0 ? current : direction > 0 ? -1 : 0;
        keyboardNode = candidates[(start + direction + candidates.length) % candidates.length];
      }
      hover = undefined;
      requestRender();
      return;
    }
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      toggleFocus(keyboardNode ?? focus ?? candidates[0]);
      return;
    }
    if (event.key === 'Escape' && focus) {
      event.preventDefault();
      toggleFocus(focus);
    }
  }

  function requestRender() {
    if (!canvas || animation) return;
    animation = requestAnimationFrame((time) => {
      animation = 0;
      draw(time);
    });
  }

  function draw(time: number) {
    if (!context) return;
    const cameraActive = advanceCamera(time);
    const ctx = context;
    ctx.clearRect(0, 0, width, height);
    ctx.save();
    ctx.translate(width / 2 + panX, height / 2 + panY);
    ctx.scale(scale, scale);
    const related = focus ? new Set(relatedNodes(focus).map((node) => node.id)) : undefined;
    const opening = reducedMotion ? 1 : ease((time - startAt) / 1250);
    edges.forEach((edge, index) => {
      const source = byId.get(edge.source);
      const target = byId.get(edge.target);
      if (!source || !target || !visible(source) || !visible(target)) return;
      const progress = reducedMotion ? 1 : ease((time - startAt - 180 - index * .32) / 900);
      if (progress <= 0) return;
      const highlighted = focus && (source === focus || target === focus);
      ctx.beginPath();
      ctx.moveTo(source.x, source.y);
      ctx.lineTo(source.x + (target.x - source.x) * progress, source.y + (target.y - source.y) * progress);
      ctx.strokeStyle = highlighted ? 'rgba(245,245,245,.72)' : edge.kind === 'depends' ? 'rgba(205,205,205,.21)' : 'rgba(170,170,170,.12)';
      ctx.lineWidth = (highlighted ? 1.35 : edge.kind === 'depends' ? .72 * linkStrength : .5) / scale;
      ctx.stroke();
    });
    for (const node of nodes) {
      if (!visible(node)) continue;
      const birth = reducedMotion ? 1 : ease((opening - node.born * .42) / .58);
      if (birth <= 0) continue;
      const isRelated = !focus || related?.has(node.id);
      const radius = nodeRadius(node);
      ctx.globalAlpha = (isRelated ? 1 : .12) * birth;
      if (node === focus) {
        ctx.beginPath();
        ctx.arc(node.x, node.y, radius + 5 / scale, 0, Math.PI * 2);
        ctx.strokeStyle = 'rgba(255,255,255,.58)';
        ctx.lineWidth = 1 / scale;
        ctx.stroke();
      } else if (focus && isRelated && !reducedMotion && time - focusAt < 1_400) {
        const wave = ((time - focusAt) % 700) / 700;
        ctx.beginPath();
        ctx.arc(node.x, node.y, radius + 3 + wave * 10, 0, Math.PI * 2);
        ctx.strokeStyle = `rgba(255,255,255,${.16 * (1 - wave)})`;
        ctx.lineWidth = .7 / scale;
        ctx.stroke();
      }
      if (canvasFocused && node === keyboardNode && node !== focus) {
        ctx.save();
        ctx.setLineDash([2 / scale, 2 / scale]);
        ctx.beginPath();
        ctx.arc(node.x, node.y, radius + 5 / scale, 0, Math.PI * 2);
        ctx.strokeStyle = 'rgba(255,255,255,.8)';
        ctx.lineWidth = 1 / scale;
        ctx.stroke();
        ctx.restore();
      }
      ctx.beginPath();
      ctx.arc(node.x, node.y, radius * birth, 0, Math.PI * 2);
      ctx.fillStyle = node === focus || node === hover || node === keyboardNode ? '#fff' : node.kind === 'directory' ? '#d9d9d9' : '#858585';
      ctx.fill();
      if (node === hover || node === focus || node === keyboardNode || (scale > 1.7 && node.kind === 'directory')) {
        ctx.font = `${11 / scale}px "Noto Serif SC", "STSong", serif`;
        ctx.fillStyle = '#e8e8e8';
        ctx.textBaseline = 'middle';
        ctx.fillText(node.label, node.x + radius + 5 / scale, node.y);
      }
    }
    ctx.globalAlpha = 1;
    ctx.restore();
    const openingActive = !reducedMotion && time - startAt < 1_900;
    const focusPulseActive = !reducedMotion && Boolean(focus) && time - focusAt < 1_400;
    if (cameraActive || openingActive || focusPulseActive) requestRender();
  }

  function updateMotionPreference(event: MediaQueryListEvent) {
    reducedMotion = event.matches;
    if (reducedMotion && cameraMotion) {
      const target = cameraMotion.to;
      cameraMotion = undefined;
      panX = target.panX;
      panY = target.panY;
      scale = target.scale;
    }
    requestRender();
  }

  onMount(() => {
    motionPreference = matchMedia('(prefers-reduced-motion: reduce)');
    reducedMotion = motionPreference.matches;
    motionPreference.addEventListener('change', updateMotionPreference);
    observer = new ResizeObserver(resize);
    observer.observe(host);
    resize();
    reset();
    canvas.addEventListener('wheel', wheel, { passive: false });
    requestRender();
  });
  onDestroy(() => {
    cancelAnimationFrame(animation);
    observer?.disconnect();
    motionPreference?.removeEventListener('change', updateMotionPreference);
    canvas?.removeEventListener('wheel', wheel);
  });
</script>

<div class:focused={Boolean(focus)} class="graph-canvas-host" bind:this={host}>
  <canvas
    bind:this={canvas}
    tabindex="0"
    on:pointerdown={down}
    on:pointermove={move}
    on:pointerup={(event) => finishPointer(event)}
    on:pointercancel={(event) => finishPointer(event, true)}
    on:pointerleave={leave}
    on:keydown={keydown}
    on:focus={() => { canvasFocused = true; keyboardNode ??= focus ?? nodes.find(visible); requestRender(); }}
    on:blur={() => { canvasFocused = false; requestRender(); }}
    aria-label={accessibleLabel}
    aria-keyshortcuts="Enter Space Escape ArrowUp ArrowDown ArrowLeft ArrowRight Home End"
  ></canvas>
  {#if hover}
    <div class="graph-tooltip" style={`left:${pointerX - (canvas?.getBoundingClientRect().left ?? 0) + 14}px;top:${pointerY - (canvas?.getBoundingClientRect().top ?? 0) + 14}px`}>
      <strong>{hover.label}</strong><span>{hover.path}</span>
    </div>
  {/if}
</div>
