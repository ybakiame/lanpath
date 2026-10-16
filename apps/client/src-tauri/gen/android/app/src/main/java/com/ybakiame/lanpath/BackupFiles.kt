package com.ybakiame.lanpath

import android.app.Activity
import android.content.ContentValues
import android.os.Environment
import android.provider.MediaStore
import android.util.Base64
import app.tauri.plugin.Invoke

/** Binary encrypted backup export adapter; plaintext never enters Downloads. */
object BackupFiles {
  fun export(activity: Activity, invoke: Invoke) {
    val args = invoke.parseArgs(ExportSubscriptionArgs::class.java)
    if (!Regex("Lanpath-backup-[a-f0-9-]+\\.zip\\.age").matches(args.filename) || args.content.length > 24 * 1024 * 1024) {
      invoke.reject("无效的备份文件")
      return
    }
    val resolver = activity.contentResolver
    var uri: android.net.Uri? = null
    try {
      val bytes = Base64.decode(args.content, Base64.NO_WRAP)
      if (bytes.size > 16 * 1024 * 1024 || !bytes.take(21).toByteArray().toString(Charsets.US_ASCII).startsWith("age-encryption.org/v1")) {
        throw IllegalArgumentException("备份格式无效")
      }
      val values = ContentValues().apply {
        put(MediaStore.MediaColumns.DISPLAY_NAME, args.filename)
        put(MediaStore.MediaColumns.MIME_TYPE, "application/octet-stream")
        put(MediaStore.MediaColumns.RELATIVE_PATH, Environment.DIRECTORY_DOWNLOADS + "/Lanpath/")
        put(MediaStore.MediaColumns.IS_PENDING, 1)
      }
      val created = resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values)
        ?: throw IllegalStateException("无法创建下载文件")
      uri = created
      resolver.openOutputStream(created)?.use { it.write(bytes) } ?: throw IllegalStateException("无法写入备份")
      if (resolver.update(created, ContentValues().apply { put(MediaStore.MediaColumns.IS_PENDING, 0) }, null, null) != 1) {
        throw IllegalStateException("无法提交下载文件")
      }
      invoke.resolveObject("Download/Lanpath/${args.filename}")
    } catch (_: Exception) {
      uri?.let { resolver.delete(it, null, null) }
      invoke.reject("备份导出失败，请检查下载目录空间")
    }
  }
}
