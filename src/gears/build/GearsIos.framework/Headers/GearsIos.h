//
//  GearsIos.h
//  GearsIos
//
//  Created by Thales Gelinger on 09/07/24.
//

#import <Foundation/Foundation.h>

//! Project version number for GearsIos.
FOUNDATION_EXPORT double GearsIosVersionNumber;

//! Project version string for GearsIos.
FOUNDATION_EXPORT const unsigned char GearsIosVersionString[];

// In this header, you should import all the public headers of your framework using statements like #import <GearsIos/PublicHeader.h>

#import <UIKit/UIKit.h>

void* createView(void);
void* createLabel(const char* text);

