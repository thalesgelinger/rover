//
//  Props.swift
//  RoverIos
//
//  Created by Thales Gelinger on 16/07/24.
//

import Foundation
import UIKit

enum HorizontalAlignment: String, Codable {
    case left, center, right
}

enum Size: Codable {
    case full
    case value(Double)

    enum CodingKeys: String, CodingKey {
        case full
        case value
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        if let value = try? container.decode(String.self)  {
            if value == "full" {
                self = .full
                return
            }
        }
        
        if let value = try? container.decode(Double.self) {
            self = .value(value)
            return
        }
        throw DecodingError.dataCorruptedError(in: container, debugDescription: "Invalid size value")
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        switch self {
        case .full:
            try container.encode(true)
        case .value(let value):
            try container.encode(value)
        }
    }
    
}

enum VerticalAlignment: String, Codable {
    case top, center, bottom
}

struct ViewProps: Codable {
    var height: Size?
    var width: Size?
    var horizontal: HorizontalAlignment?
    var vertical: VerticalAlignment?
    var color: String?
    
    init() {
        self.horizontal = nil
        self.vertical = nil
        self.color = nil
        self.height = nil
        self.width = nil
    }
    
    enum CodingKeys: String, CodingKey {
        case height, width, horizontal, vertical, color
    }
    
    public static func fromJSON(_ json: String) -> ViewProps? {
        let data = json.data(using: .utf8)!
        let decoder = JSONDecoder()
        
        do {
            let props = try decoder.decode(ViewProps.self, from: data)
            return props
        } catch {
            print("Error decoding JSON: \(error)")
            return nil
        }
    }
}

struct TextProps: Codable {
    var color: String?
    
    init() {
        self.color = nil
    }
}
