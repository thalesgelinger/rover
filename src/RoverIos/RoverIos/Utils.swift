//
//  Utils.swift
//  RoverIos
//
//  Created by Thales Gelinger on 16/07/24.
//

import Foundation
import UIKit

class Utils {
    
    static func getHeightValue(_ size: Size?) -> Double {
        switch size {
        case .full:
            return UIScreen.main.bounds.size.height
        case .value(let val):
            return val
        default:
            return 0.0
        }
    }

    static func getWidthValue(_ size: Size?) -> Double {
        switch size {
        case .full:
            return UIScreen.main.bounds.size.width
        case .value(let val):
            return val
        default:
            return 0.0
        }
    }
}
