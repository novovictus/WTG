#include <windows.h>

#include <iostream>
#include <string>

#include "SDK/ADLXHelper/Windows/Cpp/ADLXHelper.h"

using namespace adlx;

static const char* ResultName(ADLX_RESULT result) {
    switch (result) {
    case ADLX_OK:
        return "ADLX_OK";
    case ADLX_ALREADY_ENABLED:
        return "ADLX_ALREADY_ENABLED";
    case ADLX_ALREADY_INITIALIZED:
        return "ADLX_ALREADY_INITIALIZED";
    case ADLX_FAIL:
        return "ADLX_FAIL";
    case ADLX_INVALID_ARGS:
        return "ADLX_INVALID_ARGS";
    case ADLX_BAD_VER:
        return "ADLX_BAD_VER";
    case ADLX_UNKNOWN_INTERFACE:
        return "ADLX_UNKNOWN_INTERFACE";
    case ADLX_TERMINATED:
        return "ADLX_TERMINATED";
    case ADLX_ADL_INIT_ERROR:
        return "ADLX_ADL_INIT_ERROR";
    case ADLX_NOT_FOUND:
        return "ADLX_NOT_FOUND";
    case ADLX_INVALID_OBJECT:
        return "ADLX_INVALID_OBJECT";
    case ADLX_ORPHAN_OBJECTS:
        return "ADLX_ORPHAN_OBJECTS";
    case ADLX_NOT_SUPPORTED:
        return "ADLX_NOT_SUPPORTED";
    case ADLX_PENDING_OPERATION:
        return "ADLX_PENDING_OPERATION";
    case ADLX_GPU_INACTIVE:
        return "ADLX_GPU_INACTIVE";
    case ADLX_GPU_IN_USE:
        return "ADLX_GPU_IN_USE";
    case ADLX_TIMEOUT_OPERATION:
        return "ADLX_TIMEOUT_OPERATION";
    case ADLX_NOT_ACTIVE:
        return "ADLX_NOT_ACTIVE";
    case ADLX_RESET_NEEDED:
        return "ADLX_RESET_NEEDED";
    default:
        return "ADLX_UNKNOWN_RESULT";
    }
}

static void PrintExportState(HMODULE module, const char* export_name) {
    FARPROC proc = GetProcAddress(module, export_name);
    std::cout << export_name << " export: " << (proc ? "present" : "missing") << std::endl;
}

int main() {
    std::cout << "AMD ADLX external control" << std::endl;

    HMODULE module = LoadLibraryW(L"amdadlx64.dll");
    if (!module) {
        std::cout << "DLL load: failed" << std::endl;
        return 1;
    }

    wchar_t module_path[MAX_PATH] = {};
    DWORD module_path_len = GetModuleFileNameW(module, module_path, MAX_PATH);
    std::wcout << L"DLL load: ok" << std::endl;
    if (module_path_len != 0) {
        std::wcout << L"DLL path: " << module_path << std::endl;
    }
    PrintExportState(module, "ADLXInitialize2");
    PrintExportState(module, "ADLXInitialize");
    PrintExportState(module, "ADLXInitializeWithIncompatibleDriver2");
    PrintExportState(module, "ADLXInitializeWithIncompatibleDriver");
    PrintExportState(module, "ADLXTerminate");

    FreeLibrary(module);

    ADLXHelper helper;
    ADLX_RESULT init_result = helper.Initialize();
    std::cout << "Helper.Initialize(): " << init_result << " (" << ResultName(init_result) << ")" << std::endl;

    IADLXSystem* system = helper.GetSystemServices();
    std::cout << "IADLXSystem returned: " << (system ? "yes" : "no") << std::endl;

    if (ADLX_SUCCEEDED(init_result) && system != nullptr) {
        IADLXGPUListPtr gpus;
        ADLX_RESULT gpu_result = system->GetGPUs(&gpus);
        std::cout << "IADLXSystem::GetGPUs(): " << gpu_result << " (" << ResultName(gpu_result) << ")" << std::endl;

        if (ADLX_SUCCEEDED(gpu_result) && gpus) {
            adlx_uint gpu_count = gpus->Size();
            std::cout << "GPU count: " << gpu_count << std::endl;
            for (adlx_uint index = 0; index < gpu_count; ++index) {
                IADLXGPUPtr gpu;
                ADLX_RESULT at_result = gpus->At(index, &gpu);
                std::cout << "GPU[" << index << "] At(): " << at_result << " (" << ResultName(at_result) << ")" << std::endl;
                if (ADLX_SUCCEEDED(at_result) && gpu) {
                    const char* name = nullptr;
                    ADLX_RESULT name_result = gpu->Name(&name);
                    std::cout << "GPU[" << index << "] Name(): " << name_result << " (" << ResultName(name_result) << ")";
                    if (ADLX_SUCCEEDED(name_result) && name != nullptr) {
                        std::cout << " -> " << name;
                    }
                    std::cout << std::endl;
                }
            }
        }
    }

    ADLX_RESULT terminate_result = helper.Terminate();
    std::cout << "Helper.Terminate(): " << terminate_result << " (" << ResultName(terminate_result) << ")" << std::endl;
    return 0;
}
