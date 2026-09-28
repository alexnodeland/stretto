/** The console's pages. The overview loads with the app; the rest load when first opened. */
import { createRouter, createWebHistory, type RouteRecordRaw } from 'vue-router'
import OverviewPage from '@/pages/OverviewPage.vue'

export const routes: RouteRecordRaw[] = [
  { path: '/', name: 'overview', component: OverviewPage },
  { path: '/servers', name: 'servers', component: () => import('@/pages/ServersPage.vue') },
  {
    path: '/servers/new',
    name: 'server-new',
    component: () => import('@/pages/ServerEditPage.vue'),
  },
  { path: '/servers/:name', name: 'server', component: () => import('@/pages/ServerPage.vue') },
  {
    path: '/servers/:name/edit',
    name: 'server-edit',
    component: () => import('@/pages/ServerEditPage.vue'),
  },
  { path: '/sessions', name: 'sessions', component: () => import('@/pages/SessionsPage.vue') },
  { path: '/sessions/:key', name: 'session', component: () => import('@/pages/SessionPage.vue') },
  { path: '/flows', name: 'flows', component: () => import('@/pages/FlowsPage.vue') },
  { path: '/flows/:key', name: 'flow', component: () => import('@/pages/FlowPage.vue') },
  { path: '/jobs', name: 'jobs', component: () => import('@/pages/JobsPage.vue') },
  { path: '/jobs/new', name: 'job-new', component: () => import('@/pages/JobNewPage.vue') },
  { path: '/jobs/:id', name: 'job', component: () => import('@/pages/JobPage.vue') },
  { path: '/settings', name: 'settings', component: () => import('@/pages/SettingsPage.vue') },
  {
    path: '/:pathMatch(.*)*',
    name: 'not-found',
    component: () => import('@/pages/NotFoundPage.vue'),
  },
]

export function makeRouter() {
  return createRouter({
    history: createWebHistory(),
    routes,
    scrollBehavior(to, from, saved) {
      if (saved) return saved
      if (to.path === from.path) return false
      return { top: 0 }
    },
  })
}
