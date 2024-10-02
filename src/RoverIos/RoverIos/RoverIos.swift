//
//  RoverIos.swift
//  RoverIos
//
//  Created by Thales Gelinger on 04/07/24.
//

import Foundation
import RoverIos.Gears
import UIKit

var devServerCallback: ((String) -> Void)?

var cCallback: Callback =  { messageRaw in
    let message = String(cString: messageRaw!)
    
    devServerCallback?(message)
}

open class RoverIos: UIViewController {
    public override func viewDidLoad() {
        super.viewDidLoad()
        print("started")
        
        devServerCallback =  { message in
            let fileManager = FileManager.default
            let directoryURL = fileManager.urls(for: .documentDirectory, in: .userDomainMask).first!
            let fullPath = directoryURL.appendingPathComponent(message)
            print(fullPath)
            DispatchQueue.main.async {
                start(self.view, fullPath.path)
            }
        }
        
        DispatchQueue.global(qos: .background).async {
            print("run dev server")
            devServer(cCallback)
        }
    }
}
