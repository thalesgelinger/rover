//
//  Mechanic.swift
//  ios
//
//  Created by Thales Gelinger on 25/06/24.
//

import Foundation

class Gears {
    func greetings(to: String) -> String {
        let result = gretting(to)
        let swift_result = String(cString: result!)
        greeting_free(UnsafeMutablePointer(mutating: result))
        return swift_result
    }
}
