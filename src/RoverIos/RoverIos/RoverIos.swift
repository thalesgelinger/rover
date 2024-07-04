//
//  RoverIos.swift
//  RoverIos
//
//  Created by Thales Gelinger on 04/07/24.
//

import Foundation
import RoverIos.Gears

public func swiftHello(_ str: String) -> String {
    let result = gretting(str)
    let swift_result = String(cString: result!)
    print("Swift Received: \(swift_result)")
    return swift_result
}
