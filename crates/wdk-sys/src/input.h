/* Copyright (c) Microsoft Corporation
   License: MIT OR Apache-2.0 */

#if defined(UMDF_VERSION_MAJOR)

#include <windows.h>

#else // !defined(UMDF_VERSION_MAJOR)

#include "ntifs.h"
#include "ntddk.h"

// HID
// FIXME: enable all of these
// Headers list from https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/_hid/
#include "hidclass.h"
//#include "hidpddi.h"
//#include "hidpi.h"
#include "hidport.h"
#include "hidsdi.h"
//#include "HidSpiCx/1.0/hidspicx.h"
#include "kbdmou.h"
#include "ntdd8042.h"
#include "vhf.h"

// USB
#include "usbspec.h"

#include "usb.h"
#include "usbbusif.h"
#include "usbioctl.h"
#include "usbdlib.h"

#include "usbfnbase.h"
#include "usbfnattach.h"
#include "usbfnioctl.h"

#include "wdf.h" // TODO: how to deal with both WDF dependency in non-wdf settings
#include "wdfusb.h"

#ifdef _KERNEL_MODE
// FIXME: gate these each as separate features
#include "ntintsafe.h"
#include "ntstrsafe.h"
#include "pepfx.h"

// FIXME: add additional storage apis: https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/_storage/
#include "ntddstor.h"

// UDE
#include "ude/1.0/UdeCx.h"
#endif

// TODO: gate properly and spb version
#include "spb.h"
#include "spb/1.1/spbcx.h"
#include "reshub.h"
#include "pwmutil.h"

// PARALLEL PORTS
#include "gpio.h"
#include "gpioclx.h"
#include "ntddpar.h"
#include "ntddser.h"
#include "parallel.h"

// FIXME: Why is there no definition for this struct? Maybe blocklist this struct in bindgen. 
typedef union _KGDTENTRY64
{
  struct
  {
    unsigned short LimitLow;
    unsigned short BaseLow;
    union
    {
      struct
      {
        unsigned char BaseMiddle;
        unsigned char Flags1;
        unsigned char Flags2;
        unsigned char BaseHigh;
      } Bytes;
      struct
      {
        unsigned long BaseMiddle : 8;
        unsigned long Type : 5;
        unsigned long Dpl : 2;
        unsigned long Present : 1;
        unsigned long LimitHigh : 4;
        unsigned long System : 1;
        unsigned long LongMode : 1;
        unsigned long DefaultBig : 1;
        unsigned long Granularity : 1;
        unsigned long BaseHigh : 8;
      } Bits;
    };
    unsigned long BaseUpper;
    unsigned long MustBeZero;
  };
  unsigned __int64 Alignment;
} KGDTENTRY64, *PKGDTENTRY64;

typedef union _KIDTENTRY64
{
  struct
  {
    unsigned short OffsetLow;
    unsigned short Selector;
    unsigned short IstIndex : 3;
    unsigned short Reserved0 : 5;
    unsigned short Type : 5;
    unsigned short Dpl : 2;
    unsigned short Present : 1;
    unsigned short OffsetMiddle;
    unsigned long OffsetHigh;
    unsigned long Reserved1;
  };
  unsigned __int64 Alignment;
} KIDTENTRY64, *PKIDTENTRY64;
#endif // !defined(UMDF_VERSION_MAJOR)

#if defined(KMDF_VERSION_MAJOR) || defined(UMDF_VERSION_MAJOR)

#include <wdf.h>

#endif // defined(KMDF_VERSION_MAJOR) || defined(UMDF_VERSION_MAJOR)

// Tracelogging
#include "TraceLoggingProvider.h"
