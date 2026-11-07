// SPDX-License-Identifier: BSD-2-Clause
import DefaultTheme from 'vitepress/theme'
import type { Theme } from 'vitepress'
import './style.css'
import BlockPartitionDemo from './components/BlockPartitionDemo.vue'
import BlockSizeChart from './components/BlockSizeChart.vue'
import CountingPassDemo from './components/CountingPassDemo.vue'
import DigitPlanner from './components/DigitPlanner.vue'
import KeyEncoder from './components/KeyEncoder.vue'
import PipelineMap from './components/PipelineMap.vue'
import RadixComparison from './components/RadixComparison.vue'
import VaryingBits from './components/VaryingBits.vue'

export default {
  extends: DefaultTheme,
  enhanceApp({ app }) {
    app.component('BlockPartitionDemo', BlockPartitionDemo)
    app.component('BlockSizeChart', BlockSizeChart)
    app.component('CountingPassDemo', CountingPassDemo)
    app.component('DigitPlanner', DigitPlanner)
    app.component('KeyEncoder', KeyEncoder)
    app.component('PipelineMap', PipelineMap)
    app.component('RadixComparison', RadixComparison)
    app.component('VaryingBits', VaryingBits)
  },
} satisfies Theme
