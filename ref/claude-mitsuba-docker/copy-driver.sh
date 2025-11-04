mkdir -p driver/usr/share/nvidia
mkdir -p driver/usr/lib/x86_64-linux-gnu
cp /usr/share/nvidia/nvoptix.bin driver/usr/share/nvidia
cp -Rdp /usr/lib/x86_64-linux-gnu/libnvoptix.so.* driver/usr/lib/x86_64-linux-gnu/
cp -Rdp /usr/lib/x86_64-linux-gnu/libnvidia-rtcore.* driver/usr/lib/x86_64-linux-gnu/
cp -Rdp /usr/lib/x86_64-linux-gnu/libnvidia-ptxjitcompiler.* driver/usr/lib/x86_64-linux-gnu/
cp -Rdp /usr/lib/x86_64-linux-gnu/libnvidia-gpucomp.* driver/usr/lib/x86_64-linux-gnu/
