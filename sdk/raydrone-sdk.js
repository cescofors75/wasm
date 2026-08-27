/* RayDrone SDK v1 — API declarativa para materiales y escenas.
 *
 * El DSP vive en WASM/AudioWorklet; por eso los paquetes de terceros describen
 * una composición segura de controles, no ejecutan código dentro del hilo de
 * audio. Esto mantiene el audio libre de asignaciones, dependencias y sorpresas
 * de tiempo real. Un host solo necesita pasar `node.port` al SDK.
 */
(function (root, factory) {
    const api = factory();
    if (typeof module === 'object' && module.exports) module.exports = api;
    root.RayDroneSDK = api;
})(typeof globalThis !== 'undefined' ? globalThis : this, function () {
    'use strict';
    const Material = Object.freeze({ VACUUM: 0, METAL: 1, WOOD: 2, CRYSTAL: 3, WATER: 4, PLASMA: 5 });
    const materials = new Map();
    const scenes = new Map();
    const finite = (value, fallback) => Number.isFinite(value) ? value : fallback;
    const clamp = (value, lo, hi, fallback = lo) => Math.max(lo, Math.min(hi, finite(value, fallback)));

    function assertName(name, type) {
        if (typeof name !== 'string' || !/^[a-z][a-z0-9-]{1,63}$/i.test(name)) throw new TypeError(`${type} name must be 2–64 letters, digits or hyphens`);
    }
    function normaliseMaterial(definition) {
        if (!definition || typeof definition !== 'object') throw new TypeError('material definition must be an object');
        return Object.freeze({
            base: (definition.base ?? Material.VACUUM) >>> 0,
            amount: clamp(definition.amount ?? 1, 0, 1, 1),
            modulation: definition.modulation ? Object.freeze({
                mode: clamp(definition.modulation.mode, 0, 2, 0) >>> 0,
                target: clamp(definition.modulation.target, 0, 2, 0) >>> 0,
                rate: clamp(definition.modulation.rate, 0.001, 30, 0.25),
                depth: clamp(definition.modulation.depth, 0, 1, 0),
                attack: clamp(definition.modulation.attack, 0.001, 10, 0.05),
                release: clamp(definition.modulation.release, 0.001, 10, 0.5),
            }) : null,
            effects: definition.effects ? Object.freeze({
                delayWet: clamp(definition.effects.delayWet, 0, 1, 0), delayTime: clamp(definition.effects.delayTime, 0.01, 1.8, .38),
                delayFeedback: clamp(definition.effects.delayFeedback, 0, .92, .35), chorusWet: clamp(definition.effects.chorusWet, 0, 1, 0),
                chorusRate: clamp(definition.effects.chorusRate, .01, 8, .25), chorusDepth: clamp(definition.effects.chorusDepth, .001, .03, .008),
                reverbWet: clamp(definition.effects.reverbWet, 0, 1, 0),
            }) : null,
        });
    }
    function registerMaterial(name, definition) { assertName(name, 'material'); const d = normaliseMaterial(definition); materials.set(name, d); return d; }
    function registerScene(name, definition) {
        assertName(name, 'scene');
        if (!definition || typeof definition !== 'object') throw new TypeError('scene definition must be an object');
        const d = Object.freeze({ material: definition.material || null, params: Object.freeze({ ...(definition.params || {}) }) });
        scenes.set(name, d); return d;
    }
    function post(port, message) {
        if (!port || typeof port.postMessage !== 'function') throw new TypeError('attach a RayDrone AudioWorklet port first');
        port.postMessage(message);
    }
    function applyMaterial(port, material) {
        const d = typeof material === 'string' ? materials.get(material) : normaliseMaterial(material);
        if (!d) throw new Error(`unknown RayDrone material: ${material}`);
        post(port, { type: 'material', kind: d.base, amount: d.amount });
        if (d.modulation) post(port, { type: 'modulation', ...d.modulation });
        if (d.effects) { post(port, { type: 'effects', ...d.effects }); post(port, { type: 'reverb', wet: d.effects.reverbWet }); }
        return d;
    }
    function applyScene(port, scene) {
        const d = typeof scene === 'string' ? scenes.get(scene) : scene;
        if (!d) throw new Error(`unknown RayDrone scene: ${scene}`);
        if (d.material) applyMaterial(port, d.material);
        if (d.params && Object.keys(d.params).length) post(port, { type: 'params', ...d.params });
        return d;
    }
    return Object.freeze({ version: '1.0.0', Material, registerMaterial, registerScene, applyMaterial, applyScene, materials, scenes });
});
