diff --git a/wtg-app/src/main.rs b/wtg-app/src/main.rs
index 3a4b5c6..7d8e9f0 100644
--- a/wtg-app/src/main.rs
+++ b/wtg-app/src/main.rs
@@ -200,6 +200,8 @@ fn parse_args() -> CliArgs {
     let mut parsed = CliArgs::default();
 
     let mut i = 1;
+    if parsed.force_config && !parsed.mqtt_save_config {
+        usage_error("--force-config is valid only with --mqtt-save-config.");
+    }
     while i < args.len() {
         match args[i].as_str() {
             "--once" => {
