import { mount } from 'svelte';
import StudioApp from './StudioApp.svelte';
import './styles.css';
import './studio.css';

mount(StudioApp, { target: document.getElementById('app')! });
