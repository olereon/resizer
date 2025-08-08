import React, { useState, useCallback, useRef } from 'react';
import { Upload, Settings, Download, Trash2, Play, Pause } from 'lucide-react';

const BatchImageResizer = () => {
  const [files, setFiles] = useState([]);
  const [resizeMode, setResizeMode] = useState('scale'); // 'scale' or 'dimension'
  const [scaleFactor, setScaleFactor] = useState(1);
  const [targetWidth, setTargetWidth] = useState(800);
  const [targetHeight, setTargetHeight] = useState(600);
  const [dimensionMode, setDimensionMode] = useState('width'); // 'width' or 'height'
  const [quality, setQuality] = useState(90);
  const [namePrefix, setNamePrefix] = useState('');
  const [nameSuffix, setNameSuffix] = useState('_resized');
  const [keepOriginalNames, setKeepOriginalNames] = useState(false);
  const [outputFolder, setOutputFolder] = useState('');
  const [processing, setProcessing] = useState(false);
  const [progress, setProgress] = useState(0);
  const [currentFile, setCurrentFile] = useState('');
  const [processedFiles, setProcessedFiles] = useState([]);
  const [errorMessage, setErrorMessage] = useState('');
  const [successCount, setSuccessCount] = useState(0);
  const [fileAnalysis, setFileAnalysis] = useState([]);
  const [showAnalysis, setShowAnalysis] = useState(false);
  const fileInputRef = useRef(null);

  const supportedFormats = ['image/jpeg', 'image/jpg', 'image/png', 'image/webp', 'image/gif'];

  const handleDragOver = useCallback((e) => {
    e.preventDefault();
  }, []);

  const handleDragEnter = useCallback((e) => {
    e.preventDefault();
  }, []);

  const handleDrop = useCallback((e) => {
    e.preventDefault();
    const droppedFiles = Array.from(e.dataTransfer.files).filter(file => 
      supportedFormats.includes(file.type)
    );
    setFiles(prev => [...prev, ...droppedFiles]);
  }, []);

  const handleFileSelect = (e) => {
    const selectedFiles = Array.from(e.target.files).filter(file => 
      supportedFormats.includes(file.type)
    );
    setFiles(prev => [...prev, ...selectedFiles]);
  };

  const removeFile = (index) => {
    setFiles(prev => prev.filter((_, i) => i !== index));
  };

  const clearAllFiles = () => {
    setFiles([]);
    setProcessedFiles([]);
    setProgress(0);
    setFileAnalysis([]);
    setShowAnalysis(false);
    setErrorMessage('');
  };

  const analyzeFile = (file) => {
    return new Promise((resolve) => {
      const reader = new FileReader();
      reader.onload = (e) => {
        const arrayBuffer = e.target.result;
        const bytes = new Uint8Array(arrayBuffer.slice(0, 8));
        
        // Check file signatures
        let actualType = 'unknown';
        if (bytes[0] === 0xFF && bytes[1] === 0xD8 && bytes[2] === 0xFF) {
          actualType = 'JPEG';
        } else if (bytes[0] === 0x89 && bytes[1] === 0x50 && bytes[2] === 0x4E && bytes[3] === 0x47) {
          actualType = 'PNG';
        } else if (bytes[0] === 0x47 && bytes[1] === 0x49 && bytes[2] === 0x46) {
          actualType = 'GIF';
        } else if (bytes[0] === 0x52 && bytes[1] === 0x49 && bytes[2] === 0x46 && bytes[3] === 0x46) {
          actualType = 'WEBP';
        }
        
        resolve({
          name: file.name,
          declaredType: file.type,
          actualType: actualType,
          size: file.size,
          sizeFormatted: formatFileSize(file.size),
          isValid: actualType !== 'unknown'
        });
      };
      reader.readAsArrayBuffer(file.slice(0, 8));
    });
  };

  const analyzeAllFiles = async () => {
    if (files.length === 0) return;
    
    setShowAnalysis(true);
    const analyses = [];
    
    for (const file of files) {
      const analysis = await analyzeFile(file);
      analyses.push(analysis);
    }
    
    setFileAnalysis(analyses);
  };

  const loadImage = (file) => {
    return new Promise((resolve, reject) => {
      try {
        console.log(`Attempting to load ${file.name} (${formatFileSize(file.size)})`);
        
        const img = new Image();
        const url = URL.createObjectURL(file);
        
        // Set a timeout to catch hanging loads
        const timeout = setTimeout(() => {
          URL.revokeObjectURL(url);
          reject(new Error(`Timeout loading image: ${file.name}. Image may be too large or complex.`));
        }, 30000); // 30 second timeout
        
        img.onload = () => {
          clearTimeout(timeout);
          URL.revokeObjectURL(url);
          console.log(`Successfully loaded ${file.name}: ${img.width}x${img.height}`);
          resolve(img);
        };
        
        img.onerror = (e) => {
          clearTimeout(timeout);
          URL.revokeObjectURL(url);
          console.error(`Image load error for ${file.name}:`, e);
          reject(new Error(`Failed to load image: ${file.name}. Image may be too large, use an unsupported JPEG variant, or exceed browser limits.`));
        };
        
        // Try to force image loading by setting crossOrigin
        img.crossOrigin = 'anonymous';
        img.src = url;
        
      } catch (error) {
        reject(new Error(`Error creating image element: ${error.message || error}`));
      }
    });
  };

  const resizeImage = (img, file) => {
    return new Promise((resolve, reject) => {
      try {
        const canvas = document.createElement('canvas');
        const ctx = canvas.getContext('2d');

        let newWidth, newHeight;

        if (resizeMode === 'scale') {
          newWidth = Math.round(img.width * scaleFactor);
          newHeight = Math.round(img.height * scaleFactor);
        } else {
          const aspectRatio = img.width / img.height;
          
          if (dimensionMode === 'width') {
            newWidth = targetWidth;
            newHeight = Math.round(targetWidth / aspectRatio);
          } else {
            newHeight = targetHeight;
            newWidth = Math.round(targetHeight * aspectRatio);
          }
        }

        canvas.width = newWidth;
        canvas.height = newHeight;

        // Use high-quality scaling
        ctx.imageSmoothingEnabled = true;
        ctx.imageSmoothingQuality = 'high';
        
        ctx.drawImage(img, 0, 0, newWidth, newHeight);

        // Convert quality percentage to decimal
        const qualityDecimal = quality / 100;
        
        canvas.toBlob((blob) => {
          if (blob) {
            resolve(blob);
          } else {
            reject(new Error('Failed to create blob'));
          }
        }, file.type, qualityDecimal);
      } catch (error) {
        reject(error);
      }
    });
  };

  const generateFileName = (originalFile) => {
    const extension = originalFile.name.split('.').pop();
    const nameWithoutExt = originalFile.name.replace(`.${extension}`, '');
    
    let finalName;
    if (keepOriginalNames) {
      finalName = originalFile.name;
    } else {
      finalName = `${namePrefix}${nameWithoutExt}${nameSuffix}.${extension}`;
    }
    
    // Add output folder as prefix if specified
    if (outputFolder.trim()) {
      const folderPrefix = outputFolder.trim().replace(/[^\w\-_]/g, '_');
      return `${folderPrefix}_${finalName}`;
    }
    
    return finalName;
  };

  const processImages = async () => {
    if (files.length === 0) return;

    setProcessing(true);
    setProgress(0);
    setProcessedFiles([]);
    setErrorMessage('');
    setSuccessCount(0);
    const processed = [];
    let successfulCount = 0;

    try {
      for (let i = 0; i < files.length; i++) {
        const file = files[i];
        setCurrentFile(file.name);
        
        try {
          console.log(`Processing ${file.name}...`);
          
          // Validate file type
          if (!supportedFormats.includes(file.type)) {
            throw new Error(`Unsupported file type: ${file.type}`);
          }
          
          // Validate file size (optional - prevent very large files from crashing)
          if (file.size > 50 * 1024 * 1024) { // 50MB limit
            throw new Error(`File too large: ${formatFileSize(file.size)}. Please use files smaller than 50MB.`);
          }
          
          console.log(`Loading image: ${file.name} (${formatFileSize(file.size)})`);
          const img = await loadImage(file);
          console.log(`Image loaded successfully: ${img.width}x${img.height}`);
          
          console.log(`Starting resize for: ${file.name}`);
          const resizedBlob = await resizeImage(img, file);
          
          if (!resizedBlob) {
            throw new Error('Failed to create resized image - blob is null or undefined');
          }
          
          console.log(`Image resized successfully, new size: ${formatFileSize(resizedBlob.size)}`);
          
          const newFileName = generateFileName(file);
          
          const processedFile = {
            blob: resizedBlob,
            name: newFileName,
            originalSize: file.size,
            newSize: resizedBlob.size,
            originalDimensions: `${img.width}x${img.height}`,
            newDimensions: resizeMode === 'scale' 
              ? `${Math.round(img.width * scaleFactor)}x${Math.round(img.height * scaleFactor)}`
              : dimensionMode === 'width' 
                ? `${targetWidth}x${Math.round(targetWidth / (img.width / img.height))}`
                : `${Math.round(targetHeight * (img.width / img.height))}x${targetHeight}`
          };
          
          processed.push(processedFile);
          successfulCount++;
          setSuccessCount(successfulCount);
          console.log(`Successfully processed ${file.name}`);
          
        } catch (error) {
          console.error(`Error processing ${file.name}:`, error);
          const errorMsg = error?.message || error?.toString() || 'Unknown error occurred';
          setErrorMessage(prev => prev + `Error processing ${file.name}: ${errorMsg}\n`);
        }
        
        const newProgress = ((i + 1) / files.length) * 100;
        setProgress(newProgress);
        
        // Add a small delay to make progress visible
        await new Promise(resolve => setTimeout(resolve, 50));
      }

      setProcessedFiles(processed);
      console.log(`Processing complete. ${processed.length} files processed successfully out of ${files.length}.`);
      
    } catch (globalError) {
      console.error('Global processing error:', globalError);
      setErrorMessage(prev => prev + `Global error: ${globalError.message}\n`);
    } finally {
      setProcessing(false);
      setCurrentFile('');
    }
  };

  const downloadFile = (processedFile) => {
    const url = URL.createObjectURL(processedFile.blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = processedFile.name;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  };

  const downloadAll = () => {
    processedFiles.forEach(file => downloadFile(file));
  };

  const formatFileSize = (bytes) => {
    if (bytes === 0) return '0 Bytes';
    const k = 1024;
    const sizes = ['Bytes', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
  };

  return (
    <div className="min-h-screen bg-gradient-to-br from-blue-50 to-indigo-100 p-4">
      <div className="max-w-6xl mx-auto">
        <div className="text-center mb-8">
          <h1 className="text-3xl font-bold text-gray-800 mb-2">Batch Image Resizer</h1>
          <p className="text-gray-600">Fast, client-side image resizing tool</p>
        </div>

        <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
          {/* Upload Section */}
          <div className="lg:col-span-2">
            <div className="bg-white rounded-lg shadow-md p-6">
              <h2 className="text-xl font-semibold mb-4 flex items-center">
                <Upload className="mr-2" size={20} />
                Upload Images
              </h2>
              
              <div
                className="border-2 border-dashed border-gray-300 rounded-lg p-8 text-center hover:border-blue-400 transition-colors"
                onDragOver={handleDragOver}
                onDragEnter={handleDragEnter}
                onDrop={handleDrop}
                onClick={() => fileInputRef.current?.click()}
              >
                <Upload className="mx-auto mb-4 text-gray-400" size={48} />
                <p className="text-lg font-medium text-gray-700 mb-2">
                  Drag & drop images here
                </p>
                <p className="text-sm text-gray-500 mb-4">
                  or click to select files
                </p>
                <p className="text-xs text-gray-400">
                  Supports: JPEG, PNG, WebP, GIF
                </p>
                <input
                  ref={fileInputRef}
                  type="file"
                  multiple
                  accept=".jpg,.jpeg,.png,.webp,.gif"
                  onChange={handleFileSelect}
                  className="hidden"
                />
              </div>

              {files.length > 0 && (
                <div className="mt-4">
                  <div className="flex justify-between items-center mb-3">
                    <h3 className="font-medium">Selected Files ({files.length})</h3>
                    <button
                      onClick={clearAllFiles}
                      className="text-red-500 hover:text-red-700 flex items-center"
                    >
                      <Trash2 size={16} className="mr-1" />
                      Clear All
                    </button>
                  </div>
                  <div className="max-h-48 overflow-y-auto space-y-2">
                    {files.map((file, index) => (
                      <div key={index} className="flex justify-between items-center bg-gray-50 p-2 rounded">
                        <div>
                          <span className="text-sm font-medium">{file.name}</span>
                          <span className="text-xs text-gray-500 ml-2">
                            {formatFileSize(file.size)}
                          </span>
                        </div>
                        <button
                          onClick={() => removeFile(index)}
                          className="text-red-500 hover:text-red-700"
                        >
                          <Trash2 size={16} />
                        </button>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </div>
          </div>

          {/* Settings Panel */}
          <div className="bg-white rounded-lg shadow-md p-6">
            <h2 className="text-xl font-semibold mb-4 flex items-center">
              <Settings className="mr-2" size={20} />
              Resize Settings
            </h2>

            <div className="space-y-4">
              <div>
                <label className="block text-sm font-medium mb-2">Resize Mode</label>
                <select
                  value={resizeMode}
                  onChange={(e) => setResizeMode(e.target.value)}
                  className="w-full p-2 border border-gray-300 rounded"
                >
                  <option value="scale">Scale Factor</option>
                  <option value="dimension">By Dimension</option>
                </select>
              </div>

              {resizeMode === 'scale' && (
                <div>
                  <label className="block text-sm font-medium mb-2">
                    Scale Factor: {scaleFactor}x
                  </label>
                  <input
                    type="range"
                    min="0.25"
                    max="6"
                    step="0.25"
                    value={scaleFactor}
                    onChange={(e) => setScaleFactor(parseFloat(e.target.value))}
                    className="w-full"
                  />
                  <div className="flex justify-between text-xs text-gray-500 mt-1">
                    <span>0.25x</span>
                    <span>6x</span>
                  </div>
                </div>
              )}

              {resizeMode === 'dimension' && (
                <div className="space-y-3">
                  <div>
                    <label className="block text-sm font-medium mb-2">Resize by</label>
                    <select
                      value={dimensionMode}
                      onChange={(e) => setDimensionMode(e.target.value)}
                      className="w-full p-2 border border-gray-300 rounded"
                    >
                      <option value="width">Width (maintain aspect)</option>
                      <option value="height">Height (maintain aspect)</option>
                    </select>
                  </div>
                  
                  {dimensionMode === 'width' && (
                    <div>
                      <label className="block text-sm font-medium mb-2">Target Width (px)</label>
                      <input
                        type="number"
                        value={targetWidth}
                        onChange={(e) => setTargetWidth(parseInt(e.target.value))}
                        className="w-full p-2 border border-gray-300 rounded"
                        min="1"
                        max="4000"
                      />
                    </div>
                  )}

                  {dimensionMode === 'height' && (
                    <div>
                      <label className="block text-sm font-medium mb-2">Target Height (px)</label>
                      <input
                        type="number"
                        value={targetHeight}
                        onChange={(e) => setTargetHeight(parseInt(e.target.value))}
                        className="w-full p-2 border border-gray-300 rounded"
                        min="1"
                        max="4000"
                      />
                    </div>
                  )}
                </div>
              )}

              <div>
                <label className="block text-sm font-medium mb-2">
                  Quality: {quality}%
                </label>
                <input
                  type="range"
                  min="80"
                  max="100"
                  step="1"
                  value={quality}
                  onChange={(e) => setQuality(parseInt(e.target.value))}
                  className="w-full"
                />
                <div className="flex justify-between text-xs text-gray-500 mt-1">
                  <span>80%</span>
                  <span>100%</span>
                </div>
              </div>

              <div>
                <label className="block text-sm font-medium mb-2">Output Organization</label>
                <input
                  type="text"
                  value={outputFolder}
                  onChange={(e) => setOutputFolder(e.target.value)}
                  className="w-full p-2 border border-gray-300 rounded"
                  placeholder="e.g., resized-images, thumbnails"
                />
                <p className="text-xs text-gray-500 mt-1">
                  Note: Files download to your browser's default folder. Use this as a prefix for organization.
                </p>
              </div>

              <div className="border-t pt-4">
                <h3 className="font-medium mb-3">File Naming</h3>
                
                <div className="mb-3">
                  <label className="flex items-center">
                    <input
                      type="checkbox"
                      checked={keepOriginalNames}
                      onChange={(e) => setKeepOriginalNames(e.target.checked)}
                      className="mr-2"
                    />
                    <span className="text-sm">Keep original names</span>
                  </label>
                </div>

                {!keepOriginalNames && (
                  <div className="space-y-2">
                    <div>
                      <label className="block text-xs font-medium mb-1">Prefix</label>
                      <input
                        type="text"
                        value={namePrefix}
                        onChange={(e) => setNamePrefix(e.target.value)}
                        className="w-full p-1 border border-gray-300 rounded text-sm"
                        placeholder="e.g., thumb_"
                      />
                    </div>
                    <div>
                      <label className="block text-xs font-medium mb-1">Suffix</label>
                      <input
                        type="text"
                        value={nameSuffix}
                        onChange={(e) => setNameSuffix(e.target.value)}
                        className="w-full p-1 border border-gray-300 rounded text-sm"
                        placeholder="e.g., _resized"
                      />
                    </div>
                  </div>
                )}
              </div>

              <div className="flex flex-wrap gap-2">
                <button
                  onClick={processImages}
                  disabled={files.length === 0 || processing}
                  className="flex-1 bg-blue-500 text-white py-2 px-4 rounded hover:bg-blue-600 disabled:bg-gray-300 disabled:cursor-not-allowed flex items-center justify-center"
                >
                  {processing ? (
                    <>
                      <Pause className="mr-2" size={16} />
                      Processing...
                    </>
                  ) : (
                    <>
                      <Play className="mr-2" size={16} />
                      Start Processing
                    </>
                  )}
                </button>
                
                {files.length > 0 && !processing && (
                  <>
                    <button
                      onClick={analyzeAllFiles}
                      className="bg-purple-500 text-white py-2 px-3 rounded hover:bg-purple-600 text-sm"
                      title="Analyze file integrity and format"
                    >
                      Analyze
                    </button>
                    
                    <button
                      onClick={() => {
                        // Process only first 5 files to test
                        const testFiles = files.slice(0, 5);
                        const originalFiles = files;
                        setFiles(testFiles);
                        setTimeout(() => {
                          processImages().finally(() => {
                            setFiles(originalFiles);
                          });
                        }, 100);
                      }}
                      className="bg-orange-500 text-white py-2 px-3 rounded hover:bg-orange-600 text-sm"
                      title="Test with first 5 files only"
                    >
                      Test 5
                    </button>
                  </>
                )}
              </div>
            </div>
          </div>
        </div>

        {/* File Analysis Section */}
        {showAnalysis && fileAnalysis.length > 0 && (
          <div className="mt-6 bg-white rounded-lg shadow-md p-6">
            <div className="flex justify-between items-center mb-4">
              <h2 className="text-xl font-semibold">File Analysis Results</h2>
              <button
                onClick={() => setShowAnalysis(false)}
                className="text-gray-500 hover:text-gray-700"
              >
                ✕
              </button>
            </div>
            
            <div className="max-h-64 overflow-y-auto space-y-2">
              {fileAnalysis.map((analysis, index) => (
                <div 
                  key={index} 
                  className={`p-3 rounded border ${
                    analysis.isValid 
                      ? 'bg-green-50 border-green-200' 
                      : 'bg-red-50 border-red-200'
                  }`}
                >
                  <div className="font-medium text-sm mb-1">
                    {analysis.name.length > 50 ? analysis.name.substring(0, 50) + '...' : analysis.name}
                  </div>
                  <div className="text-xs space-y-1">
                    <div>
                      <span className="font-medium">Declared Type:</span> {analysis.declaredType || 'Unknown'}
                    </div>
                    <div>
                      <span className="font-medium">Actual Type:</span> 
                      <span className={analysis.isValid ? 'text-green-600' : 'text-red-600'}>
                        {' ' + analysis.actualType}
                      </span>
                    </div>
                    <div>
                      <span className="font-medium">Size:</span> {analysis.sizeFormatted}
                    </div>
                    {!analysis.isValid && (
                      <div className="text-red-600 font-medium">
                        ⚠️ File appears to be corrupted or not a valid image
                      </div>
                    )}
                  </div>
                </div>
              ))}
            </div>
            
            <div className="mt-4 p-3 bg-blue-50 rounded">
              <h3 className="font-medium text-blue-800 mb-2">Analysis Summary:</h3>
              <div className="text-sm text-blue-700">
                <div>Valid Images: {fileAnalysis.filter(f => f.isValid).length}</div>
                <div>Invalid/Corrupted: {fileAnalysis.filter(f => !f.isValid).length}</div>
                <div>Total Size: {formatFileSize(fileAnalysis.reduce((sum, f) => sum + f.size, 0))}</div>
              </div>
            </div>
          </div>
        )}

        {/* Progress Section */}
        {(processing || processedFiles.length > 0 || errorMessage) && (
          <div className="mt-6 bg-white rounded-lg shadow-md p-6">
            <h2 className="text-xl font-semibold mb-4">Processing Status</h2>
            
            {processing && (
              <div className="mb-4">
                <div className="flex justify-between text-sm mb-2">
                  <span>Processing: {currentFile}</span>
                  <span>{Math.round(progress)}% ({successCount}/{files.length})</span>
                </div>
                <div className="w-full bg-gray-200 rounded-full h-2">
                  <div
                    className="bg-blue-500 h-2 rounded-full transition-all duration-300"
                    style={{ width: `${progress}%` }}
                  />
                </div>
              </div>
            )}

            {errorMessage && (
              <div className="mb-4 p-3 bg-red-50 border border-red-200 rounded">
                <h3 className="font-medium text-red-800 mb-2">Processing Errors:</h3>
                <pre className="text-sm text-red-600 whitespace-pre-wrap">{errorMessage}</pre>
              </div>
            )}

            {processedFiles.length > 0 && !processing && (
              <div>
                <div className="flex justify-between items-center mb-4">
                  <h3 className="font-medium text-green-700">
                    Successfully Processed: {processedFiles.length} out of {files.length} images
                  </h3>
                  <button
                    onClick={downloadAll}
                    className="bg-green-500 text-white px-4 py-2 rounded hover:bg-green-600 flex items-center"
                  >
                    <Download className="mr-2" size={16} />
                    Download All ({processedFiles.length})
                  </button>
                </div>
                
                <div className="max-h-64 overflow-y-auto space-y-2">
                  {processedFiles.map((file, index) => (
                    <div key={index} className="bg-gray-50 p-3 rounded flex justify-between items-center">
                      <div>
                        <div className="font-medium text-sm">{file.name}</div>
                        <div className="text-xs text-gray-500">
                          {file.originalDimensions} → {file.newDimensions} | 
                          {formatFileSize(file.originalSize)} → {formatFileSize(file.newSize)}
                        </div>
                      </div>
                      <button
                        onClick={() => downloadFile(file)}
                        className="text-blue-500 hover:text-blue-700 p-1"
                        title="Download this file"
                      >
                        <Download size={16} />
                      </button>
                    </div>
                  ))}
                </div>
              </div>
            )}
            
            {!processing && processedFiles.length === 0 && files.length > 0 && errorMessage && (
              <div className="text-center p-4">
                <p className="text-red-600 font-medium">No images were processed successfully.</p>
                <p className="text-sm text-gray-600 mt-2">
                  Please check the error messages above and try with different images.
                </p>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
};

export default BatchImageResizer;