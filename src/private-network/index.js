// @ts-check
const { ipcMain, shell, dialog } = require('electron')
const { join } = require('path')
const fs = require('fs-extra')
const i18n = require('i18next')
const os = require('os')
const path = require('path')
const store = require('../common/store')
const logger = require('../common/logger')
const { generateSwarmKey, importSwarmKey, validateSwarmKey, readSwarmKey } = require('../daemon/config')
const { kuboApiPost } = require('../common/kubo-rpc')
const ipcMainEvents = require('../common/ipc-main-events')
const getCtx = require('../context')

/**
 * Get the IPFS repository path (same logic as tray.js getKuboRepositoryPath).
 *
 * @returns {string}
 */
function getRepoPath () {
  let ipfsPath = store.get('ipfsConfig.path')
  if (!ipfsPath) {
    ipfsPath = process.env.IPFS_PATH
    if (!ipfsPath) {
      const homeDir = os.homedir()
      ipfsPath = path.join(homeDir, '.ipfs')
    }
  }
  return ipfsPath
}

/**
 * Get the swarm.key file path for the current repository.
 *
 * @returns {string}
 */
function getSwarmKeyPath () {
  return join(getRepoPath(), 'swarm.key')
}

/**
 * Check if the current repository has a swarm.key (i.e. is in a private network).
 *
 * @returns {boolean}
 */
function hasSwarmKey () {
  return fs.pathExistsSync(getSwarmKeyPath())
}

/**
 * Get swarm key info for display.
 *
 * @returns {{ keyContent: string, keyPath: string } | null}
 */
function getSwarmKeyInfo () {
  const repoPath = getRepoPath()
  const result = readSwarmKey(repoPath)
  if (!result) return null
  return {
    keyContent: result.keyContent,
    keyPath: join(repoPath, 'swarm.key')
  }
}

/**
 * List connected peers via the Kubo RPC API.
 * Returns an array of peer addresses.
 *
 * @param {import('ipfsd-ctl').Controller} ipfsd
 * @returns {Promise<{peers: import('multiaddr').Multiaddr[], count: number}>}
 */
async function listPeers (ipfsd) {
  try {
    const peers = await ipfsd.api.swarm.peers()
    return {
      peers: peers.map(p => p.peer),
      count: peers.length
    }
  } catch (err) {
    logger.error(`[private-network] failed to list peers: ${err.message}`)
    return { peers: [], count: 0 }
  }
}

/**
 * Generate a new swarm key for the current repository.
 *
 * @returns {{ keyContent: string, keyPath: string } | null}
 */
function generateSwarmKeyForRepo () {
  const repoPath = getRepoPath()
  if (hasSwarmKey()) {
    logger.info('[private-network] swarm.key already exists, skipping generation')
    return getSwarmKeyInfo()
  }
  return generateSwarmKey(repoPath)
}

/**
 * Import a swarm key into the current repository.
 *
 * @param {string} keyContent
 * @returns {{ keyContent: string, keyPath: string } | null}
 */
function importSwarmKeyToRepo (keyContent) {
  if (!validateSwarmKey(keyContent)) {
    throw new Error('Invalid swarm key format: must be 64 hex characters')
  }
  const repoPath = getRepoPath()
  return importSwarmKey(repoPath, keyContent)
}

/**
 * Setup IPC handlers for private network features.
 */
function setupIpcHandlers () {
  ipcMain.handle('private-network:hasSwarmKey', async () => {
    return hasSwarmKey()
  })

  ipcMain.handle('private-network:getSwarmKeyInfo', async () => {
    return getSwarmKeyInfo()
  })

  ipcMain.handle('private-network:generateSwarmKey', async () => {
    try {
      const result = generateSwarmKeyForRepo()
      logger.info('[private-network] swarm.key generated successfully')
      return { success: true, ...result }
    } catch (err) {
      logger.error(`[private-network] failed to generate swarm.key: ${err.message}`)
      return { success: false, error: err.message }
    }
  })

  ipcMain.handle('private-network:importSwarmKey', async (_event, keyContent) => {
    try {
      const result = importSwarmKeyToRepo(keyContent)
      logger.info('[private-network] swarm.key imported successfully')
      return { success: true, ...result }
    } catch (err) {
      logger.error(`[private-network] failed to import swarm.key: ${err.message}`)
      return { success: false, error: err.message }
    }
  })

  ipcMain.handle('private-network:listPeers', async () => {
    try {
      const getIpfsd = await getCtx().getProp('getIpfsd')
      const ipfsd = await getIpfsd(true)
      if (!ipfsd) {
        return { success: false, error: 'IPFS daemon is not running', peers: [], count: 0 }
      }
      const result = await listPeers(ipfsd)
      logger.info(`[private-network] listed ${result.count} peers`)
      return { success: true, ...result }
    } catch (err) {
      logger.error(`[private-network] failed to list peers: ${err.message}`)
      return { success: false, error: err.message, peers: [], count: 0 }
    }
  })

  ipcMain.handle('private-network:openSwarmKeyLocation', async () => {
    const swarmKeyPath = getSwarmKeyPath()
    if (fs.pathExistsSync(swarmKeyPath)) {
      shell.showItemInFolder(swarmKeyPath)
      return { success: true }
    }
    return { success: false, error: 'swarm.key not found' }
  })

  logger.info('[private-network] IPC handlers registered')
}

/**
 * Clean up the swarm.key file from the repository.
 *
 * @returns {boolean} true if removed successfully
 */
function removeSwarmKey () {
  const swarmKeyPath = getSwarmKeyPath()
  if (fs.pathExistsSync(swarmKeyPath)) {
    fs.removeSync(swarmKeyPath)
    logger.info(`[private-network] removed swarm.key from ${swarmKeyPath}`)
    return true
  }
  return false
}

module.exports = {
  setupIpcHandlers,
  hasSwarmKey,
  getSwarmKeyInfo,
  generateSwarmKeyForRepo,
  importSwarmKeyToRepo,
  listPeers,
  removeSwarmKey,
  getRepoPath,
  getSwarmKeyPath
}
