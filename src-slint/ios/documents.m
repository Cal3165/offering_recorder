#import <UIKit/UIKit.h>
#import <UniformTypeIdentifiers/UniformTypeIdentifiers.h>
#import <objc/runtime.h>

@interface OfferingDocumentDelegate : NSObject <UIDocumentPickerDelegate, UIAdaptivePresentationControllerDelegate>
@property(nonatomic, copy) void (^completion)(NSURL *url);
@end

@implementation OfferingDocumentDelegate
- (void)finish:(NSURL *)url {
    void (^completion)(NSURL *) = self.completion;
    self.completion = nil;
    if (completion) completion(url);
}

- (void)documentPicker:(UIDocumentPickerViewController *)controller
    didPickDocumentsAtURLs:(NSArray<NSURL *> *)urls {
    [self finish:urls.firstObject];
}

- (void)documentPickerWasCancelled:(UIDocumentPickerViewController *)controller {
    [self finish:nil];
}

- (void)presentationControllerDidDismiss:(UIPresentationController *)controller {
    [self finish:nil];
}
@end

static UIViewController *offering_presenter(void) {
    for (UIScene *scene in UIApplication.sharedApplication.connectedScenes) {
        if (![scene isKindOfClass:UIWindowScene.class] ||
            scene.activationState != UISceneActivationStateForegroundActive) continue;
        UIViewController *controller = ((UIWindowScene *)scene).keyWindow.rootViewController;
        while (controller.presentedViewController) controller = controller.presentedViewController;
        if (controller) return controller;
    }
    return nil;
}

static int copy_csv(NSURL *source, NSURL *destination) {
    BOOL scoped = [source startAccessingSecurityScopedResource];
    NSFileCoordinator *coordinator = [[NSFileCoordinator alloc] initWithFilePresenter:nil];
    __block int result = -1;
    [coordinator coordinateReadingItemAtURL:source options:0 error:NULL byAccessor:^(NSURL *url) {
        NSFileHandle *handle = [NSFileHandle fileHandleForReadingFromURL:url error:NULL];
        const NSUInteger limit = 5 * 1024 * 1024;
        if (!handle) return;
        NSMutableData *data = [NSMutableData data];
        while (data.length <= limit) {
            NSData *part = [handle readDataUpToLength:MIN((NSUInteger)16384, limit + 1 - data.length)
                                               error:NULL];
            if (!part) break;
            if (part.length == 0) {
                if ([data writeToURL:destination options:NSDataWritingAtomic error:NULL]) result = 1;
                break;
            }
            [data appendData:part];
        }
        [handle closeAndReturnError:NULL];
        if (data.length > limit) result = -2;
    }];
    if (scoped) [source stopAccessingSecurityScopedResource];
    return result;
}

int offering_pick_document(const char *path, int exporting) {
    @autoreleasepool {
        if (NSThread.isMainThread) return -1;
        NSString *file = [NSString stringWithUTF8String:path];
        if (!file) return -1;
        NSURL *staged = [NSURL fileURLWithPath:file];
        dispatch_semaphore_t done = dispatch_semaphore_create(0);
        __block NSURL *selection = nil;
        __block BOOL unavailable = NO;

        dispatch_async(dispatch_get_main_queue(), ^{
            UIViewController *presenter = offering_presenter();
            if (!presenter) {
                unavailable = YES;
                dispatch_semaphore_signal(done);
                return;
            }
            UIDocumentPickerViewController *picker;
            if (exporting) {
                picker = [[UIDocumentPickerViewController alloc] initForExportingURLs:@[staged] asCopy:YES];
            } else {
                picker = [[UIDocumentPickerViewController alloc]
                    initForOpeningContentTypes:@[UTTypeCommaSeparatedText, UTTypePlainText] asCopy:NO];
            }
            OfferingDocumentDelegate *delegate = [OfferingDocumentDelegate new];
            delegate.completion = ^(NSURL *url) {
                selection = url;
                dispatch_semaphore_signal(done);
            };
            // UIKit's delegate references are weak. The presented picker owns this delegate.
            static char delegateKey;
            objc_setAssociatedObject(picker, &delegateKey, delegate, OBJC_ASSOCIATION_RETAIN_NONATOMIC);
            picker.delegate = delegate;
            picker.presentationController.delegate = delegate;
            [presenter presentViewController:picker animated:YES completion:nil];
        });
        dispatch_semaphore_wait(done, DISPATCH_TIME_FOREVER);
        if (unavailable) return -1;
        if (!selection) return 0;
        return exporting ? 1 : copy_csv(selection, staged);
    }
}
