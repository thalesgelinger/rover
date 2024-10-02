//
//  FileUtils.swift
//  RoverIos
//
//  Created by Thales Gelinger on 26/09/24.
//

import Foundation

@objc public class FileUtils: NSObject {

    @objc public static func createFolderIfNotExists(_ folderName: String) -> String {
        let fileManager = FileManager.default
        let directoryURL = fileManager.urls(for: .documentDirectory, in: .userDomainMask).first!
        let folderURL = directoryURL.appendingPathComponent(folderName)

        do {
            if !fileManager.fileExists(atPath: folderURL.path) {
                try fileManager.createDirectory(at: folderURL, withIntermediateDirectories: true, attributes: nil)
                print("Directory created successfully at path: \(folderURL.path)")
            } else {
                print("Directory already exists at path: \(folderURL.path)")
            }
        } catch {
            print("Failed to create directory: \(error.localizedDescription)")
            return ""
        }

        return folderURL.path
    }

    // Write file to a given path
    @objc public static func writeFile(_ path: String, content: String) -> String {
        let fileManager = FileManager.default
        let directoryURL = fileManager.urls(for: .documentDirectory, in: .userDomainMask).first!
        let fileURL = directoryURL.appendingPathComponent(path)

        let parentDirURL = fileURL.deletingLastPathComponent()

        // Ensure the directory exists, create if necessary
        do {
            if !fileManager.fileExists(atPath: parentDirURL.path) {
                try fileManager.createDirectory(at: parentDirURL, withIntermediateDirectories: true, attributes: nil)
            }

            // Write the content to the file
            try content.write(to: fileURL, atomically: true, encoding: .utf8)
            print("File written successfully at path: \(fileURL.path)")
        } catch {
            print("Failed to write file: \(error.localizedDescription)")
            return ""  // Handle error case, return failure indicator
        }

        return fileURL.path  // Return the file path where content was written
    }
}
